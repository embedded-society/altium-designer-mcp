//! `PcbLib` text and fill primitives.

#[allow(clippy::wildcard_imports)] // sibling primitive types
use super::*;

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

/// Text justification (alignment). Shared with `SchLib`; the canonical
/// definition is [`crate::altium::TextJustification`]. A `PcbLib` text's
/// text-box anchor defaults to `BottomLeft`, which encodes to the geometry
/// template's justification byte (`0x03` = Altium `LeftBottom`) at offset 132 —
/// so a from-scratch text stays byte-identical. This matches `AltiumSharp`'s
/// `PcbText.Justification` default.
pub use crate::altium::TextJustification;

/// Default text-box justification for a from-scratch `PcbLib` text: `BottomLeft`,
/// which the writer encodes to the template's `0x03` byte at geometry offset 132.
#[allow(clippy::trivially_copy_pass_by_ref)] // serde requires &T
const fn is_zero_barcode_kind(v: &u8) -> bool {
    *v == 0
}

const fn default_justification() -> TextJustification {
    TextJustification::BottomLeft
}

/// Default font name for a from-scratch text: `"Arial"`, matching the geometry
/// template's UTF-16 font-name field (offsets 46-109).
fn default_font_name() -> String {
    "Arial".to_string()
}

/// Default net index for a from-scratch text/fill (`0xFFFF` = no net). The
/// common-header connectivity indices default to "none" so a free library
/// primitive writes the same `0xFF` header bytes as before (byte-identity).
const fn default_net_index() -> u16 {
    0xFFFF
}

/// Default polygon index for a from-scratch text/fill (`0xFFFF` = none).
const fn default_polygon_index() -> u16 {
    0xFFFF
}

/// Default component index for a from-scratch text/fill (`-1` = free primitive,
/// stored as the `0xFFFF` common-header sentinel).
const fn default_component_index() -> i32 {
    -1
}

/// A text string on a layer.
// Altium's text record carries several independent boolean style/knockout flags
// (mirror, bold, italic, is_inverted, use_inverted_rectangle) that each map to a
// distinct byte in the fixed 252-byte SubRecord-1 layout; they are not a state
// machine, so the excessive-bools lint does not apply.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    /// X position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x: f64,
    /// Y position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y: f64,
    /// Text content.
    pub text: String,
    /// Text height in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub height: f64,
    /// The header layer byte exactly as read, kept only when the layer table
    /// does not map it — the primitive then sits on the `MultiLayer`
    /// catch-all, and without the byte a rewrite would store `74` in its
    /// place. Replayed while the primitive still sits there; moving it to a
    /// layer the model can name discards the byte for that layer's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_layer_id: Option<u8>,
    /// Layer the text is on.
    pub layer: Layer,
    /// Rotation angle in degrees.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub rotation: f64,
    /// Text rendering kind (Stroke, TrueType, or `BarCode`).
    #[serde(default)]
    pub kind: TextKind,
    /// Stroke font type. It draws the text while `kind` is `Stroke`; a
    /// TrueType or barcode text keeps the one it switches back to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_font: Option<StrokeFont>,
    /// TrueType italic style (Altium `FontItalic`, geometry offset 45). Only
    /// meaningful when `kind` is `TrueType`. `false` (the from-scratch default)
    /// reproduces the template byte exactly.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    /// Font bold style (Altium `FontBold`, geometry offset 44 — the twin of
    /// [`Self::italic`]@45). Only meaningful when `kind` is `TrueType`. `false`
    /// (the from-scratch default) reproduces the template's `0x00` byte exactly.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    /// Whether the text is mirrored (Altium `IsMirrored`, geometry offset 35;
    /// bottom-side silkscreen). `false` (the from-scratch default) reproduces the
    /// template's `0x00` byte exactly.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mirror: bool,
    /// Whether this text is the component's Comment field (Altium `IsComment`,
    /// geometry offset 40). Altium sets this on board-level comment strings so
    /// the component's `CommentOn` visibility toggle applies. `false` (the
    /// from-scratch default) reproduces the template's `0x00` byte exactly;
    /// every fixture text carries `0x00` here.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_comment: bool,
    /// Whether this text is the component's Designator field (Altium
    /// `IsDesignator`, geometry offset 41 — the twin of [`Self::is_comment`]@40).
    /// `false` (the from-scratch default) reproduces the template's `0x00` byte.
    /// Note the `.Designator` special string works through its content alone;
    /// this flag is a separate visibility marker that round-trips faithfully.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_designator: bool,
    /// TrueType font name (Altium `FontName`, geometry offset 46, UTF-16, 64-byte
    /// field). Only meaningful when `kind` is `TrueType`. Defaults to `"Arial"`,
    /// matching the template; a from-scratch default text reproduces the template's
    /// exact 64-byte UTF-16 encoding.
    #[serde(default = "default_font_name")]
    pub font_name: String,
    /// Stroke line width in mm (Altium `StrokeWidth`, geometry offset 36). `None`
    /// uses Altium's template default (4 mil); a read value round-trips exactly.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::altium::serde_round::option::serialize"
    )]
    pub stroke_width: Option<f64>,
    /// Text-box justification / anchor (Altium `InvertedRectJustification`,
    /// geometry offset 132). Defaults to `BottomLeft`, which reproduces the
    /// template byte `0x03`.
    #[serde(default = "default_justification")]
    pub justification: TextJustification,
    /// Whether the text is drawn as inverted (knockout) — a filled bar with the
    /// glyphs punched out (Altium `IsInverted`, geometry offset 110). `false`
    /// (the from-scratch default) reproduces the template's `0x00` byte.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_inverted: bool,
    /// Border margin around inverted text in mm (Altium `InvertedBorder`,
    /// geometry offset 111, i32 internal units). `None` (the from-scratch
    /// default) reproduces the template's zero bytes; a read value round-trips.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::altium::serde_round::option::serialize"
    )]
    pub inverted_border: Option<f64>,
    /// Whether the inverted text uses an explicit framed rectangle (with the
    /// `inverted_rect_*` dimensions) rather than auto-sizing to the glyphs
    /// (Altium `UseInvertedRectangle`, geometry offset 123). `false` (the
    /// from-scratch default) reproduces the template's `0x00` byte.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub use_inverted_rectangle: bool,
    /// Inverted-rectangle width in mm (Altium `InvertedRectWidth`, geometry
    /// offset 124, i32 internal units). Only meaningful when
    /// [`Self::use_inverted_rectangle`] is set. `None` (the from-scratch default)
    /// leaves the template's precomputed text-box width untouched, so a plain
    /// text stays byte-identical; a framed text round-trips its explicit width.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::altium::serde_round::option::serialize"
    )]
    pub inverted_rect_width: Option<f64>,
    /// Inverted-rectangle height in mm (Altium `InvertedRectHeight`, geometry
    /// offset 128, i32 internal units). Only meaningful when
    /// [`Self::use_inverted_rectangle`] is set. `None` (the from-scratch default)
    /// leaves the template's precomputed text-box height untouched (byte-identity).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::altium::serde_round::option::serialize"
    )]
    pub inverted_rect_height: Option<f64>,
    /// Text offset within the inverted rectangle in mm (Altium
    /// `InvertedRectTextOffset`, geometry offset 133, i32 internal units — the
    /// twin of [`Self::justification`]@132). `None` (the from-scratch default)
    /// reproduces the template's zero bytes; a read value round-trips.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "crate::altium::serde_round::option::serialize"
    )]
    pub inverted_rect_text_offset: Option<f64>,
    /// Barcode overall width in mm — geometry offset 137. Only meaningful when
    /// [`Self::kind`] is `BarCode`; `None` replays the template bytes.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub barcode_full_width: Option<f64>,
    /// Barcode overall height in mm — geometry offset 141.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub barcode_full_height: Option<f64>,
    /// Barcode horizontal quiet-zone margin in mm — geometry offset 145.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub barcode_x_margin: Option<f64>,
    /// Barcode vertical quiet-zone margin in mm — geometry offset 149.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub barcode_y_margin: Option<f64>,
    /// Barcode symbology — geometry byte @157. `0` on a non-barcode text, `1` for
    /// Code128, the only symbology AD24 names in its scripting enum.
    #[serde(default, skip_serializing_if = "is_zero_barcode_kind")]
    pub barcode_kind: u8,
    /// Font for the barcode's human-readable line — geometry offsets 161-224,
    /// stored UTF-16LE and null-padded, unlike the record's other strings.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub barcode_font_name: String,
    /// Whether the barcode renders inverted (light bars on dark) — geometry byte
    /// @159. Only meaningful for a barcode.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub barcode_inverted: bool,
    /// Whether the human-readable line is drawn under the bars — geometry byte
    /// @225. Altium defaults this on for a new barcode.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub barcode_show_text: bool,
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
    /// Net index into the board's net list — common-header u16 @3. `0xFFFF`
    /// (65535) means "no net", the from-scratch default (round-trip fidelity).
    #[serde(default = "default_net_index")]
    pub net_index: u16,
    /// Polygon index this text belongs to — common-header u16 @5. `0xFFFF`
    /// (none) from scratch, matching the historical writer output.
    #[serde(default = "default_polygon_index")]
    pub polygon_index: u16,
    /// Component index into the board's component list — common-header u16 @7
    /// (`0xFFFF` stored, exposed as `-1`). `-1` (free primitive) from scratch.
    #[serde(default = "default_component_index")]
    pub component_index: i32,
    /// Unique ID assigned by Altium (8-character alphanumeric string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
    /// Altium's stable identity for this primitive, from the footprint's
    /// `PrimitiveGuids` stream (`{XXXXXXXX-…}`), or `None` for a primitive
    /// with no recorded identity (anything built from scratch). Riding on the
    /// primitive itself, the identity follows it through structural edits —
    /// deleting a neighbour cannot re-point it, which the old footprint-level
    /// ordinal list could not guarantee.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    /// The text's geometry block exactly as read (base64 in JSON), used as
    /// the write-side base with every typed field overlaid — AD caches its own
    /// render metrics in bytes we do not model, and the golden zeroes bytes
    /// the `AltiumSharp` template fills. `None` (from scratch) uses the
    /// template.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::base64_opt"
    )]
    pub raw_geometry: Option<Vec<u8>>,
}

impl Text {
    /// Creates a stroke-font text of `height` mm at (`x`, `y`) on `layer`,
    /// every other field at the value a text placed from scratch gets.
    #[must_use]
    pub fn new(x: f64, y: f64, text: impl Into<String>, height: f64, layer: Layer) -> Self {
        Self {
            raw_layer_id: None,
            barcode_full_width: None,
            barcode_full_height: None,
            barcode_x_margin: None,
            barcode_y_margin: None,
            barcode_kind: 0,
            barcode_font_name: String::new(),
            barcode_inverted: false,
            barcode_show_text: false,
            x,
            y,
            text: text.into(),
            height,
            layer,
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
        }
    }
}

/// A filled rectangle on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    /// First corner X position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x1: f64,
    /// First corner Y position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y1: f64,
    /// Second corner X position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub x2: f64,
    /// Second corner Y position in mm.
    #[serde(serialize_with = "crate::altium::serde_round::serialize")]
    pub y2: f64,
    /// The header layer byte exactly as read, kept only when the layer table
    /// does not map it — the primitive then sits on the `MultiLayer`
    /// catch-all, and without the byte a rewrite would store `74` in its
    /// place. Replayed while the primitive still sits there; moving it to a
    /// layer the model can name discards the byte for that layer's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_layer_id: Option<u8>,
    /// Layer the fill is on.
    pub layer: Layer,
    /// Rotation angle in degrees.
    #[serde(default, serialize_with = "crate::altium::serde_round::serialize")]
    pub rotation: f64,
    /// Primitive flags (locked, keepout, etc.).
    #[serde(default, skip_serializing_if = "PcbFlags::is_empty")]
    pub flags: PcbFlags,
    /// Net index into the board's net list — common-header u16 @3. `0xFFFF`
    /// (65535) means "no net", the from-scratch default (round-trip fidelity).
    #[serde(default = "default_net_index")]
    pub net_index: u16,
    /// Polygon index this fill belongs to — common-header u16 @5. `0xFFFF`
    /// (none) from scratch, matching the historical writer output.
    #[serde(default = "default_polygon_index")]
    pub polygon_index: u16,
    /// Component index into the board's component list — common-header u16 @7
    /// (`0xFFFF` stored, exposed as `-1`). `-1` (free primitive) from scratch.
    #[serde(default = "default_component_index")]
    pub component_index: i32,
    /// Solder-mask expansion override in mm (geometry offset 37). `None` uses the
    /// rule default; round-trips like the Track/Arc extended tail.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::altium::serde_round::option"
    )]
    pub solder_mask_expansion: Option<f64>,
    /// Keepout restriction bitmask (geometry offset 46). `None` = zero on disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keepout_restrictions: Option<u8>,
    /// Unique ID assigned by Altium (8-character alphanumeric string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,
    /// Altium's stable identity for this primitive, from the footprint's
    /// `PrimitiveGuids` stream (`{XXXXXXXX-…}`), or `None` for a primitive
    /// with no recorded identity (anything built from scratch). Riding on the
    /// primitive itself, the identity follows it through structural edits —
    /// deleting a neighbour cannot re-point it, which the old footprint-level
    /// ordinal list could not guarantee.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
}

impl Fill {
    /// Creates a new Fill from corner coordinates.
    #[must_use]
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64, layer: Layer) -> Self {
        Self {
            raw_layer_id: None,
            x1,
            y1,
            x2,
            y2,
            layer,
            rotation: 0.0,
            flags: PcbFlags::empty(),
            net_index: default_net_index(),
            polygon_index: default_polygon_index(),
            component_index: default_component_index(),
            solder_mask_expansion: None,
            keepout_restrictions: None,
            unique_id: None,
            guid: None,
        }
    }

    /// Creates a Fill from centre position and dimensions.
    #[must_use]
    pub fn from_center(x: f64, y: f64, width: f64, height: f64, layer: Layer) -> Self {
        let half_w = width / 2.0;
        let half_h = height / 2.0;
        Self {
            raw_layer_id: None,
            x1: x - half_w,
            y1: y - half_h,
            x2: x + half_w,
            y2: y + half_h,
            layer,
            rotation: 0.0,
            flags: PcbFlags::empty(),
            net_index: default_net_index(),
            polygon_index: default_polygon_index(),
            component_index: default_component_index(),
            solder_mask_expansion: None,
            keepout_restrictions: None,
            unique_id: None,
            guid: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Fill, Layer, StrokeFont, Text, TextJustification, TextKind};

    #[test]
    fn fill_from_center_computes_symmetric_corners() {
        let f = Fill::from_center(1.0, 2.0, 4.0, 2.0, Layer::TopLayer);
        assert!((f.x1 - -1.0).abs() < 1e-9);
        assert!((f.y1 - 1.0).abs() < 1e-9);
        assert!((f.x2 - 3.0).abs() < 1e-9);
        assert!((f.y2 - 3.0).abs() < 1e-9);
        assert_eq!(f.layer, Layer::TopLayer);
        assert!((f.rotation - 0.0).abs() < 1e-9);
    }

    #[test]
    fn text_kind_serde_round_trips_every_variant() {
        for k in [TextKind::Stroke, TextKind::TrueType, TextKind::BarCode] {
            let s = serde_json::to_string(&k).unwrap();
            assert_eq!(serde_json::from_str::<TextKind>(&s).unwrap(), k);
        }
    }

    #[test]
    fn stroke_font_serde_round_trips_every_variant() {
        for font in [
            StrokeFont::Default,
            StrokeFont::SansSerif,
            StrokeFont::Serif,
        ] {
            let s = serde_json::to_string(&font).unwrap();
            assert_eq!(serde_json::from_str::<StrokeFont>(&s).unwrap(), font);
        }
    }

    #[test]
    fn omitted_text_fields_fall_back_to_the_template_defaults() {
        // A caller-supplied text carries only the five required fields. The
        // omitted font and justification must land on the values that reproduce
        // the geometry template byte-for-byte: "Arial" and BottomLeft (0x03).
        let json = serde_json::json!({
            "x": 1.0,
            "y": 2.0,
            "text": "REF",
            "height": 1.5,
            "layer": serde_json::to_value(Layer::TopOverlay).unwrap(),
        });
        let t: Text = serde_json::from_value(json).unwrap();
        assert_eq!(t.font_name, "Arial");
        assert_eq!(t.justification, TextJustification::BottomLeft);
    }
}
