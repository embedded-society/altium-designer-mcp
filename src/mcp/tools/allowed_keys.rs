//! The JSON keys the write tools accept, one list per object kind.
//!
//! `read_pcblib` and `read_schlib` serialise every primitive straight from
//! its struct, so a write tool that accepts exactly the struct's serialised
//! fields — plus the authoring-only spellings its parser understands —
//! replays any read unchanged, and anything else is a typo to refuse rather
//! than ignore. Wherever serde can hand over a struct's field table (every
//! `PcbLib` struct; the `SchLib` symbol, pin and footprint link) the list is
//! derived from it here and cannot drift from the struct. The `SchLib`
//! graphics flatten their display flags, which serde deserialises as a map
//! with no field table, so their lists are spelled out below and held to the
//! structs by test.

use crate::altium::{pcblib, schlib};

/// Captures the field table a derived `Deserialize` impl hands to
/// `deserialize_struct`: the exact key set the read tools emit for a struct,
/// renames applied, skipped-when-default fields included.
struct FieldTable(std::cell::Cell<Option<&'static [&'static str]>>);

impl<'de> serde::Deserializer<'de> for &FieldTable {
    type Error = serde::de::value::Error;

    fn deserialize_any<V: serde::de::Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
        Err(serde::de::Error::custom("field-table probe"))
    }

    fn deserialize_struct<V: serde::de::Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Self::Error> {
        self.0.set(Some(fields));
        Err(serde::de::Error::custom("field-table probe"))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map enum identifier ignored_any
    }
}

/// The serialised field names of a struct with a derived `Deserialize` and
/// no flattened field — every key a read tool can emit for it.
///
/// # Panics
///
/// If `T` deserialises as anything but a plain struct (a flattened field
/// turns the derived impl into a map visitor with no field table).
pub fn serde_field_names<T: serde::de::DeserializeOwned>() -> &'static [&'static str] {
    let probe = FieldTable(std::cell::Cell::new(None));
    let _ = T::deserialize(&probe);
    probe
        .0
        .get()
        .expect("a derived struct Deserialize without flatten calls deserialize_struct")
}

/// A struct's field table plus the authoring-only keys its parser accepts.
fn with_extras(fields: &[&'static str], extras: &[&'static str]) -> Vec<&'static str> {
    fields.iter().chain(extras).copied().collect()
}

/// The key lists `write_pcblib` accepts.
pub struct PcbLibKeys {
    /// A footprint object; `step_model` is the authoring spelling of `model_3d`.
    pub footprint: Vec<&'static str>,
    pub pad: Vec<&'static str>,
    pub track: Vec<&'static str>,
    pub arc: Vec<&'static str>,
    pub via: Vec<&'static str>,
    pub fill: Vec<&'static str>,
    pub region: Vec<&'static str>,
    pub text: Vec<&'static str>,
    pub component_body: Vec<&'static str>,
    /// A `step_model`/`model_3d` object; `embed` is authoring-only.
    pub model: Vec<&'static str>,
}

impl PcbLibKeys {
    pub fn new() -> Self {
        Self {
            footprint: with_extras(serde_field_names::<pcblib::Footprint>(), &["step_model"]),
            pad: serde_field_names::<pcblib::Pad>().to_vec(),
            track: serde_field_names::<pcblib::Track>().to_vec(),
            arc: serde_field_names::<pcblib::Arc>().to_vec(),
            via: serde_field_names::<pcblib::Via>().to_vec(),
            fill: serde_field_names::<pcblib::Fill>().to_vec(),
            region: serde_field_names::<pcblib::Region>().to_vec(),
            text: serde_field_names::<pcblib::Text>().to_vec(),
            component_body: serde_field_names::<pcblib::ComponentBody>().to_vec(),
            model: with_extras(serde_field_names::<pcblib::Model3D>(), &["embed"]),
        }
    }
}

/// The key lists `write_schlib` accepts for the objects serde can describe.
pub struct SchLibKeys {
    /// A symbol object; `designator_prefix` and `component_type` are the
    /// authoring routes to a designator.
    pub symbol: Vec<&'static str>,
    pub pin: Vec<&'static str>,
    /// A footprint link (`footprints[]`), not an embedded footprint.
    pub footprint: Vec<&'static str>,
}

impl SchLibKeys {
    pub fn new() -> Self {
        Self {
            symbol: with_extras(
                serde_field_names::<schlib::Symbol>(),
                &["designator_prefix", "component_type"],
            ),
            pin: serde_field_names::<schlib::Pin>().to_vec(),
            footprint: serde_field_names::<schlib::FootprintModel>().to_vec(),
        }
    }
}

/// Spells out one `SchLib` graphic's key list: its own fields, the display
/// flags, the `raw_params` replay carrier, then its authoring-only extras.
macro_rules! graphic_keys {
    ($(#[$doc:meta])* $name:ident = [$($key:literal),* $(,)?] $(+ [$($extra:literal),* $(,)?])?) => {
        $(#[$doc])*
        pub const $name: &[&str] = &[
            $($key,)*
            "graphically_locked",
            "disabled",
            "dimmed",
            "owner_part_display_mode",
            "raw_params",
            $($($extra,)*)?
        ];
    };
}

graphic_keys!(
    RECTANGLE = [
        "x1",
        "y1",
        "x2",
        "y2",
        "line_width",
        "line_color",
        "fill_color",
        "filled",
        "line_style",
        "transparent",
        "owner_part_id",
        "unique_id",
    ]
);
graphic_keys!(
    ROUND_RECT = [
        "x1",
        "y1",
        "x2",
        "y2",
        "corner_x_radius",
        "corner_y_radius",
        "line_width",
        "line_color",
        "fill_color",
        "filled",
        "line_style",
        "transparent",
        "owner_part_id",
        "unique_id",
    ]
);
graphic_keys!(
    LINE = [
        "x1",
        "y1",
        "x2",
        "y2",
        "line_width",
        "color",
        "line_style",
        "is_not_accessible",
        "owner_part_id",
        "unique_id",
    ]
);
graphic_keys!(
    /// `vertices` is the authoring spelling of `points`.
    POLYLINE = [
        "points", "line_width", "color", "line_style", "start_line_shape", "end_line_shape",
        "line_shape_size", "transparent", "is_not_accessible", "owner_part_id", "unique_id",
    ] + ["vertices"]
);
graphic_keys!(
    /// `vertices` is the authoring spelling of `points`.
    POLYGON = [
        "points", "line_width", "line_color", "fill_color", "filled", "line_style",
        "transparent", "is_not_accessible", "owner_part_id", "unique_id",
    ] + ["vertices"]
);
graphic_keys!(
    ARC = [
        "x",
        "y",
        "radius",
        "start_angle",
        "end_angle",
        "line_width",
        "color",
        "fill_color",
        "is_not_accessible",
        "owner_part_id",
        "unique_id",
    ]
);
graphic_keys!(
    PIE = [
        "x",
        "y",
        "radius",
        "start_angle",
        "end_angle",
        "line_width",
        "line_color",
        "fill_color",
        "filled",
        "transparent",
        "is_not_accessible",
        "owner_part_id",
        "unique_id",
    ]
);
graphic_keys!(
    IMAGE = [
        "x1",
        "y1",
        "x2",
        "y2",
        "line_width",
        "line_color",
        "line_style",
        "fill_color",
        "filled",
        "transparent",
        "show_border",
        "keep_aspect",
        "embed_image",
        "file_name",
        "image_data",
        "image_compressed",
        "is_not_accessible",
        "owner_part_id",
        "unique_id",
    ]
);
graphic_keys!(
    TEXT_FRAME = [
        "x1",
        "y1",
        "x2",
        "y2",
        "text",
        "color",
        "area_color",
        "text_color",
        "text_margin",
        "line_width",
        "line_style",
        "transparent",
        "font_id",
        "orientation",
        "alignment",
        "is_solid",
        "show_border",
        "word_wrap",
        "clip_to_rect",
        "is_not_accessible",
        "owner_part_id",
        "unique_id",
    ]
);
graphic_keys!(
    BEZIER = [
        "x1",
        "y1",
        "x2",
        "y2",
        "x3",
        "y3",
        "x4",
        "y4",
        "line_width",
        "color",
        "is_not_accessible",
        "owner_part_id",
        "unique_id",
    ]
);
graphic_keys!(
    ELLIPSE = [
        "x",
        "y",
        "radius_x",
        "radius_y",
        "line_width",
        "line_color",
        "fill_color",
        "filled",
        "transparent",
        "is_not_accessible",
        "owner_part_id",
        "unique_id",
    ]
);
graphic_keys!(
    /// `hidden` is the authoring spelling of `is_hidden`.
    LABEL = [
        "x", "y", "text", "color", "font_id", "is_hidden", "is_mirrored", "justification",
        "rotation", "owner_part_id", "unique_id",
    ] + ["hidden"]
);
graphic_keys!(
    PARAMETER = [
        "name",
        "value",
        "x",
        "y",
        "hidden",
        "font_id",
        "color",
        "read_only_state",
        "param_type",
        "orientation",
        "justification",
        "show_name",
        "hide_name",
        "description",
        "is_configurable",
        "is_mirrored",
        "auto_position",
        "is_rule",
        "is_system_parameter",
        "text_horz_anchor",
        "text_vert_anchor",
        "owner_part_id",
        "unique_id",
    ]
);

graphic_keys!(
    /// An elliptical arc.
    ELLIPTICAL_ARC = [
        "x",
        "y",
        "radius",
        "secondary_radius",
        "start_angle",
        "end_angle",
        "line_width",
        "color",
        "fill_color",
        "owner_part_id",
        "unique_id",
    ]
);

graphic_keys!(
    /// An IEEE symbol (record 3).
    IEEE_SYMBOL = [
        "x",
        "y",
        "symbol",
        "scale_factor",
        "rotation",
        "is_mirrored",
        "line_width",
        "color",
        "owner_part_id",
    ]
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::altium::{PcbLib, SchLib};
    use std::collections::BTreeSet;

    /// The keys a value's JSON object carries.
    fn keys_of(value: &serde_json::Value) -> BTreeSet<String> {
        value
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Every key serde emits for any item of `items`, as a sorted set.
    fn emitted_keys<T: serde::Serialize>(items: impl Iterator<Item = T>) -> BTreeSet<String> {
        items
            .flat_map(|item| keys_of(&serde_json::to_value(item).unwrap()))
            .collect()
    }

    fn assert_accepts(list: &[&str], emitted: &BTreeSet<String>, what: &str) {
        let refused: Vec<_> = emitted
            .iter()
            .filter(|k| !list.contains(&k.as_str()))
            .collect();
        assert!(
            refused.is_empty(),
            "{what}: read tools emit {refused:?}, which the writer refuses"
        );
    }

    #[test]
    fn serde_field_names_reports_the_struct_field_table() {
        let fields = serde_field_names::<pcblib::Model3D>();
        assert_eq!(
            fields,
            ["filepath", "x_offset", "y_offset", "z_offset", "rotation"]
        );
        // Skipped-when-default fields are in the table too.
        assert!(serde_field_names::<pcblib::Text>().contains(&"barcode_font_name"));
        assert!(serde_field_names::<schlib::Symbol>().contains(&"primitive_order"));
    }

    #[test]
    #[should_panic(expected = "without flatten")]
    fn serde_field_names_refuses_a_flattened_struct() {
        let _ = serde_field_names::<schlib::Rectangle>();
    }

    #[test]
    fn pcblib_lists_are_the_struct_fields_plus_authoring_spellings() {
        let keys = PcbLibKeys::new();
        for field in serde_field_names::<pcblib::Footprint>() {
            assert!(keys.footprint.contains(field), "{field}");
        }
        assert!(keys.footprint.contains(&"step_model"));
        assert!(keys.model.contains(&"embed"));
        assert!(keys.pad.contains(&"per_layer_sizes"));
        assert!(keys.text.contains(&"barcode_font_name"));
        assert!(keys.component_body.contains(&"outline"));
    }

    #[test]
    fn schlib_lists_carry_the_symbol_authoring_routes() {
        let keys = SchLibKeys::new();
        for extra in [
            "designator_prefix",
            "component_type",
            "name",
            "pins",
            "primitive_order",
        ] {
            assert!(keys.symbol.contains(&extra), "{extra}");
        }
        assert!(keys.pin.contains(&"electrical_type"));
        for field in [
            "name",
            "description",
            "library_path",
            "is_current",
            "unique_id",
        ] {
            assert!(keys.footprint.contains(&field), "{field}");
        }
    }

    /// Every graphic that flattens `ShapeDisplayFlags` accepts each flag —
    /// so a new universal flag cannot be emitted by the reader yet refused.
    #[test]
    fn every_schlib_graphic_list_accepts_the_display_flags() {
        let flags = serde_field_names::<schlib::ShapeDisplayFlags>();
        for (name, list) in [
            ("rectangle", RECTANGLE),
            ("round_rect", ROUND_RECT),
            ("line", LINE),
            ("polyline", POLYLINE),
            ("polygon", POLYGON),
            ("arc", ARC),
            ("pie", PIE),
            ("image", IMAGE),
            ("text_frame", TEXT_FRAME),
            ("bezier", BEZIER),
            ("ellipse", ELLIPSE),
            ("label", LABEL),
            ("parameter", PARAMETER),
        ] {
            for flag in flags {
                assert!(list.contains(flag), "{name} refuses {flag}");
            }
        }
    }

    /// Everything the golden `SchLib` makes the reader emit is accepted by the
    /// writer, kind by kind — the check serde cannot do for flattened structs.
    #[test]
    fn golden_schlib_emits_only_keys_the_writer_accepts() {
        let lib = SchLib::open("scripts/samples/symbols.SchLib").unwrap();
        let symbols: Vec<_> = lib.iter().collect();
        let keys = SchLibKeys::new();
        assert_accepts(&keys.symbol, &emitted_keys(symbols.iter()), "symbol");
        assert_accepts(
            &keys.pin,
            &emitted_keys(symbols.iter().flat_map(|s| &s.pins)),
            "pin",
        );
        macro_rules! check {
            ($($field:ident => $list:expr),* $(,)?) => {
                $(
                    let emitted = emitted_keys(symbols.iter().flat_map(|s| &s.$field));
                    assert!(!emitted.is_empty(), "the golden exercises {}", stringify!($field));
                    assert_accepts($list, &emitted, stringify!($field));
                )*
            };
        }
        check!(
            rectangles => RECTANGLE,
            round_rects => ROUND_RECT,
            lines => LINE,
            polylines => POLYLINE,
            polygons => POLYGON,
            arcs => ARC,
            pies => PIE,
            images => IMAGE,
            text_frames => TEXT_FRAME,
            beziers => BEZIER,
            ellipses => ELLIPSE,
            elliptical_arcs => ELLIPTICAL_ARC,
            ieee_symbols => IEEE_SYMBOL,
            labels => LABEL,
            parameters => PARAMETER,
        );
    }

    /// The same for the golden `PcbLib` — true by construction for the
    /// struct fields, so this pins the hand-listed authoring spellings and
    /// the reader's footprint-level keys.
    #[test]
    fn golden_pcblib_emits_only_keys_the_writer_accepts() {
        let lib = PcbLib::open("scripts/samples/footprints.PcbLib").unwrap();
        let fps: Vec<_> = lib.iter().collect();
        let keys = PcbLibKeys::new();
        assert_accepts(&keys.footprint, &emitted_keys(fps.iter()), "footprint");
        assert_accepts(
            &keys.pad,
            &emitted_keys(fps.iter().flat_map(|f| &f.pads)),
            "pad",
        );
        assert_accepts(
            &keys.track,
            &emitted_keys(fps.iter().flat_map(|f| &f.tracks)),
            "track",
        );
        assert_accepts(
            &keys.arc,
            &emitted_keys(fps.iter().flat_map(|f| &f.arcs)),
            "arc",
        );
        assert_accepts(
            &keys.via,
            &emitted_keys(fps.iter().flat_map(|f| &f.vias)),
            "via",
        );
        assert_accepts(
            &keys.fill,
            &emitted_keys(fps.iter().flat_map(|f| &f.fills)),
            "fill",
        );
        assert_accepts(
            &keys.region,
            &emitted_keys(fps.iter().flat_map(|f| &f.regions)),
            "region",
        );
        assert_accepts(
            &keys.text,
            &emitted_keys(fps.iter().flat_map(|f| &f.text)),
            "text",
        );
        assert_accepts(
            &keys.component_body,
            &emitted_keys(fps.iter().flat_map(|f| &f.component_bodies)),
            "component_body",
        );
        assert_accepts(
            &keys.model,
            &emitted_keys(fps.iter().filter_map(|f| f.model_3d.as_ref())),
            "model_3d",
        );
    }

    /// Every key a write or update tool accepts is described — with its
    /// type — by the tool's own schema, and every key the schema describes
    /// is one the tool accepts. The dispatch type check can only refuse a
    /// wrong-typed value under a key the schema knows, so an accepted key
    /// the schema leaves out is a value nobody checks; a described key the
    /// parser refuses is a lie in `tools/list`.
    #[test]
    #[allow(clippy::too_many_lines)] // one table row per schema object
    fn every_accepted_key_is_described_by_the_tool_schema() {
        use crate::mcp::server::McpServer;
        use crate::mcp::tools::{batch, maintenance};
        use serde_json::Value;
        use std::fmt::Write as _;

        let schemas: std::collections::HashMap<String, Value> = McpServer::get_tool_definitions()
            .into_iter()
            .map(|t| (t.name, t.input_schema))
            .collect();
        let pcb = PcbLibKeys::new();
        let sch = SchLibKeys::new();

        let footprint_pointers: Vec<(&str, Vec<&str>)> = vec![
            ("", pcb.footprint.clone()),
            ("/properties/pads/items", pcb.pad.clone()),
            ("/properties/vias/items", pcb.via.clone()),
            ("/properties/tracks/items", pcb.track.clone()),
            ("/properties/arcs/items", pcb.arc.clone()),
            ("/properties/fills/items", pcb.fill.clone()),
            ("/properties/regions/items", pcb.region.clone()),
            ("/properties/text/items", pcb.text.clone()),
            (
                "/properties/component_bodies/items",
                pcb.component_body.clone(),
            ),
            ("/properties/step_model", pcb.model.clone()),
        ];
        let symbol_pointers: Vec<(&str, Vec<&str>)> = vec![
            ("", sch.symbol.clone()),
            ("/properties/pins/items", sch.pin.clone()),
            ("/properties/footprints/items", sch.footprint.clone()),
            ("/properties/parameters/items", PARAMETER.to_vec()),
            ("/properties/rectangles/items", RECTANGLE.to_vec()),
            ("/properties/round_rects/items", ROUND_RECT.to_vec()),
            ("/properties/lines/items", LINE.to_vec()),
            ("/properties/polylines/items", POLYLINE.to_vec()),
            ("/properties/polygons/items", POLYGON.to_vec()),
            ("/properties/arcs/items", ARC.to_vec()),
            ("/properties/pies/items", PIE.to_vec()),
            ("/properties/images/items", IMAGE.to_vec()),
            ("/properties/text_frames/items", TEXT_FRAME.to_vec()),
            ("/properties/beziers/items", BEZIER.to_vec()),
            ("/properties/ellipses/items", ELLIPSE.to_vec()),
            ("/properties/labels/items", LABEL.to_vec()),
            ("/properties/elliptical_arcs/items", ELLIPTICAL_ARC.to_vec()),
            ("/properties/ieee_symbols/items", IEEE_SYMBOL.to_vec()),
        ];

        let mut checks: Vec<(String, Vec<&str>)> = Vec::new();
        for (pointer, keys) in &footprint_pointers {
            checks.push((
                format!("write_pcblib:/properties/footprints/items{pointer}"),
                keys.clone(),
            ));
            checks.push((
                format!("update_component:/properties/footprint{pointer}"),
                keys.clone(),
            ));
        }
        for (pointer, keys) in &symbol_pointers {
            checks.push((
                format!("write_schlib:/properties/symbols/items{pointer}"),
                keys.clone(),
            ));
            checks.push((
                format!("update_component:/properties/symbol{pointer}"),
                keys.clone(),
            ));
        }
        checks.push((
            "update_pad:/properties/updates".to_string(),
            maintenance::UPDATE_PAD_KEYS.to_vec(),
        ));
        let mut primitive_keys: Vec<&str> = maintenance::UPDATE_PRIMITIVE_KINDS
            .iter()
            .flat_map(|kind| maintenance::update_primitive_keys(kind).unwrap())
            .copied()
            .collect();
        primitive_keys.sort_unstable();
        primitive_keys.dedup();
        checks.push((
            "update_primitive:/properties/updates".to_string(),
            primitive_keys,
        ));
        let mut batch_keys: Vec<&str> = ["update_track_width", "rename_layer", "update_parameters"]
            .iter()
            .flat_map(|op| batch::batch_parameter_keys(op).unwrap())
            .copied()
            .collect();
        batch_keys.sort_unstable();
        batch_keys.dedup();
        checks.push((
            "batch_update:/properties/parameters".to_string(),
            batch_keys,
        ));

        let mut report = String::new();
        for (target, accepted) in &checks {
            let (tool, pointer) = target.split_once(':').unwrap();
            let Some(schema) = schemas[tool].pointer(pointer) else {
                let _ = writeln!(report, "{target}: no such schema path");
                continue;
            };
            let described: BTreeSet<&str> = schema["properties"]
                .as_object()
                .map(|p| p.keys().map(String::as_str).collect())
                .unwrap_or_default();
            let accepted: BTreeSet<&str> = accepted.iter().copied().collect();
            let undocumented: Vec<&&str> = accepted.difference(&described).collect();
            let unaccepted: Vec<&&str> = described.difference(&accepted).collect();
            if !undocumented.is_empty() {
                let _ = writeln!(
                    report,
                    "{target}: accepted but not in the schema: {undocumented:?}"
                );
            }
            if !unaccepted.is_empty() {
                let _ = writeln!(
                    report,
                    "{target}: in the schema but not accepted: {unaccepted:?}"
                );
            }
            for key in accepted.intersection(&described) {
                if schema["properties"][key].get("type").is_none() {
                    let _ = writeln!(report, "{target}: '{key}' has no type in the schema");
                }
            }
        }
        assert!(
            report.is_empty(),
            "schema and parser disagree:{}{report}",
            '\n'
        );

        // `update_component` takes the very object the write tools take.
        for (component, tool, list) in [
            ("footprint", "write_pcblib", "footprints"),
            ("symbol", "write_schlib", "symbols"),
        ] {
            let update = &schemas["update_component"]["properties"][component];
            let write = &schemas[tool]["properties"][list]["items"];
            assert_eq!(update["properties"], write["properties"], "{component}");
            assert_eq!(update["required"], write["required"], "{component}");
        }
    }
}
