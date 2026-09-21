//! JSON -> Altium primitive parsing helpers, split from `server.rs`.
//!
//! These extend `McpServer` with an additional `impl` block; the handlers in
//! other modules call them via `Self::parse_*` unchanged.

use serde_json::Value;

use crate::altium::pcblib::Footprint;
use crate::altium::schlib::Symbol;
use crate::mcp::server::{ErrorContext, McpServer, ToolCallResult};
use crate::mcp::tools::allowed_keys;

/// Maps a free-text component type to its reference-designator class letter,
/// following the conventions of IEEE 315 / ASME Y14.44 (commercial usage).
///
/// Used as the fallback when a symbol is written without an explicit
/// `designator_prefix`. Unknown or unspecified types resolve to `"U"`
/// (integrated circuit / inseparable assembly), the most common case.
// The explicit IC/regulator arm shares the `"U"` body with the wildcard
// fallback; it is kept to document the recognised IC synonyms rather than
// silently folding them into `_`.
#[allow(clippy::match_same_arms)]
pub fn ieee_designator_prefix(component_type: &str) -> &'static str {
    match component_type.trim().to_ascii_lowercase().as_str() {
        "resistor" | "res" | "potentiometer" | "pot" | "trimmer" | "rheostat" => "R",
        "resistor_network" | "resistor_array" | "network" => "RN",
        "thermistor" | "ntc" | "ptc" => "RT",
        "varistor" | "mov" => "RV",
        "capacitor" | "cap" => "C",
        "inductor" | "coil" | "choke" | "ferrite" | "ferrite_bead" | "bead" => "L",
        "diode" | "rectifier" | "schottky" | "zener" | "tvs" | "led" => "D",
        "display" | "lamp" | "indicator" | "lightbulb" => "DS",
        "transistor" | "mosfet" | "fet" | "bjt" | "igbt" | "jfet" => "Q",
        "ic" | "integrated_circuit" | "microcircuit" | "opamp" | "mcu" | "regulator"
        | "voltage_regulator" => "U",
        "connector" | "header" | "jack" | "receptacle" => "J",
        "plug" => "P",
        "socket" => "X",
        "crystal" | "oscillator" | "resonator" | "xtal" => "Y",
        "switch" | "button" | "pushbutton" | "dip_switch" | "dipswitch" => "S",
        "relay" | "contactor" => "K",
        "transformer" => "T",
        "fuse" => "F",
        "filter" => "FL",
        "battery" | "cell" => "BT",
        "test_point" | "testpoint" => "TP",
        "terminal_block" | "terminal" => "TB",
        "speaker" | "loudspeaker" | "buzzer" => "LS",
        "microphone" => "MK",
        "motor" | "fan" | "blower" => "B",
        "module" | "assembly" | "subassembly" => "A",
        "mechanical" | "standoff" | "screw" | "mounting" => "MP",
        "jumper" | "wire" | "cable" => "W",
        _ => "U",
    }
}

/// Reads a JSON integer field as `i32`, returning `None` if it is missing, not
/// an integer, or outside `i32` range — so an out-of-range value is rejected
/// rather than silently wrapped (`as i32`), which would let an absurd input
/// land as a small in-range coordinate that bypasses range validation.
fn json_i32(json: &Value, field: &str) -> Option<i32> {
    json.get(field)
        .and_then(Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
}

/// Reads a JSON number field as `f64`, accepting both integer and fractional
/// JSON values and rejecting non-finite (NaN/∞) inputs. Schematic graphic
/// coordinates use this so off-grid (fractional) positions survive instead of
/// being dropped by the integer-only [`json_i32`].
fn json_f64(json: &Value, field: &str) -> Option<f64> {
    json.get(field)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
}

/// Reads the four universal display/lock flags shared by every `SchLib` graphic
/// shape (`graphically_locked` / `disabled` / `dimmed` /
/// `owner_part_display_mode`) from a shape's JSON. Absent keys default to
/// `false` / `0`, matching Altium's omit-when-default records.
fn parse_schlib_display_flags(json: &Value) -> crate::altium::schlib::ShapeDisplayFlags {
    #[allow(clippy::cast_possible_truncation)]
    crate::altium::schlib::ShapeDisplayFlags {
        graphically_locked: json
            .get("graphically_locked")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        disabled: json
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        dimmed: json.get("dimmed").and_then(Value::as_bool).unwrap_or(false),
        owner_part_display_mode: json_i32(json, "owner_part_display_mode").unwrap_or(0),
    }
}

/// Reads the optional `flags` field of a `PcbLib` 2D primitive.
///
/// `read_pcblib` serialises [`crate::altium::pcblib::PcbFlags`] (a `bitflags`
/// set) via its serde impl, which in JSON is a string of `|`-separated flag
/// names, e.g. `"LOCKED"` or `"LOCKED | KEEPOUT"`. The write side deserialises
/// that exact form with serde so a value read from disk round-trips unchanged.
/// For caller convenience a raw `u16` bitmask integer is also accepted
/// (`1` = `LOCKED`, `4` = `KEEPOUT`, …). An absent or unparseable value yields
/// the empty flag set rather than erroring, matching the lenient handling of the
/// other optional tail fields.
fn json_flags(json: &Value) -> crate::altium::pcblib::PcbFlags {
    use crate::altium::pcblib::PcbFlags;
    match json.get("flags") {
        // Canonical round-trip shape: the bitflags serde string ("LOCKED | …").
        Some(v @ Value::String(_)) => {
            serde_json::from_value(v.clone()).unwrap_or_else(|_| PcbFlags::empty())
        }
        // Convenience shape: a raw bitmask integer.
        Some(Value::Number(n)) => n
            .as_u64()
            .and_then(|v| u16::try_from(v).ok())
            .map_or_else(PcbFlags::empty, PcbFlags::from_bits_truncate),
        _ => PcbFlags::empty(),
    }
}

/// Reads the optional `keepout_restrictions` bitmask (`u8`) of a `PcbLib` 2D
/// primitive, mirroring how `read_pcblib` serialises the `Option<u8>` field.
fn json_keepout(json: &Value) -> Option<u8> {
    json.get("keepout_restrictions")
        .and_then(Value::as_u64)
        .and_then(|v| u8::try_from(v).ok())
}

/// Reads the optional common-header net index (u16) of a `PcbLib` primitive,
/// defaulting to `0xFFFF` ("no net") — the from-scratch value the writer's
/// header fill emits. Mirrors how `read_pcblib` serialises the `net_index` field.
fn json_net_index(json: &Value) -> u16 {
    json.get("net_index")
        .and_then(Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .unwrap_or(0xFFFF)
}

/// Reads the optional common-header polygon index (u16) of a `PcbLib` primitive,
/// defaulting to `0xFFFF` (none) — the from-scratch value.
fn json_polygon_index(json: &Value) -> u16 {
    json.get("polygon_index")
        .and_then(Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .unwrap_or(0xFFFF)
}

/// Reads the optional common-header component index (i32) of a `PcbLib`
/// primitive, defaulting to `-1` (free primitive; stored as the `0xFFFF`
/// sentinel) — the from-scratch value.
fn json_component_index(json: &Value) -> i32 {
    json.get("component_index")
        .and_then(Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
        .unwrap_or(-1)
}

/// Reads the optional `unique_id` (identity GUID) of any primitive.
///
/// `read_pcblib` / `read_schlib` surface each primitive's 8-char Altium unique
/// ID via serde, so an AI doing a read-modify-write can pass it straight back
/// here to preserve stable primitive identity across saves (Altium tracks
/// primitives by this GUID for ECO). An absent value yields `None`, letting the
/// writer auto-generate a fresh GUID exactly as it does for from-scratch output.
fn json_unique_id(json: &Value) -> Option<String> {
    json.get("unique_id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Reads a `SchLib` record's raw segments (`raw_params`) from its JSON, so a
/// read-modify-write replays the record as Altium wrote it.
fn json_raw_params(json: &Value) -> Vec<(String, String)> {
    json.get("raw_params")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Reads an Altium identity GUID (`key`) from a record's JSON: absent is
/// `Ok(None)`; the braced form `read_pcblib` emits, or any spelling of the
/// same 32 hex digits, is kept verbatim; anything else is refused by name,
/// since the writer could only drop the record's identity or invent a
/// fresh one in its place. `field` names the record and key in the error.
fn guid_field(json: &Value, key: &str, field: &str) -> Result<Option<String>, String> {
    match json.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            let bare = text.trim_start_matches('{').trim_end_matches('}');
            if uuid::Uuid::parse_str(bare).is_ok() {
                Ok(Some(text.clone()))
            } else {
                Err(format!(
                    "{field} '{text}' is not a GUID ({{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}})"
                ))
            }
        }
        Some(other) => Err(format!("{field} must be a string, got {other}")),
    }
}

/// The streams a symbol carries verbatim (`extra_streams`), in the
/// `[[name, base64], …]` form `read_schlib` emits; anything else is no
/// streams, like every other carrier this parser reads leniently.
fn json_extra_streams(json: &Value) -> Vec<(String, Vec<u8>)> {
    #[derive(serde::Deserialize)]
    struct Streams(#[serde(with = "crate::altium::base64_opt::named")] Vec<(String, Vec<u8>)>);
    json.get("extra_streams")
        .cloned()
        .and_then(|v| serde_json::from_value::<Streams>(v).ok())
        .map_or_else(Vec::new, |streams| streams.0)
}

/// A read primitive's header layer byte (`raw_layer_id`), carried so the
/// rewrite keeps an unmapped byte while the primitive still sits on the
/// Multi-Layer catch-all it decoded to.
fn json_raw_layer_id(json: &Value) -> Option<u8> {
    json.get("raw_layer_id")
        .and_then(Value::as_u64)
        .and_then(|v| u8::try_from(v).ok())
}

/// Reads a base64-encoded byte field: the raw replay bases `read_pcblib`
/// emits (a pad's `raw_tail`, a via's `raw_block`, a text's `raw_geometry`)
/// and embedded image bytes. Passing one back through the tool layer keeps
/// the write byte-identical to the source block. Invalid base64 is treated
/// as absent (this parser is lenient Option-style throughout) with a debug
/// log for diagnosis; the writer then falls back to its captured template,
/// so the write still succeeds semantically.
fn json_base64(json: &Value, key: &str) -> Option<Vec<u8>> {
    json.get(key).and_then(Value::as_str).and_then(|s| {
        use base64::Engine as _;
        match base64::engine::general_purpose::STANDARD.decode(s.as_bytes()) {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                tracing::debug!(error = %e, key, "invalid base64; ignoring");
                None
            }
        }
    })
}

/// Reads an optional verbatim string field from a primitive's JSON.
fn json_guidless_opt(json: &Value, key: &str) -> Option<String> {
    json.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Accepted pad-shape spellings, quoted in every shape error so the two tools
/// give identical guidance.
/// The values an enum-valued string field accepts, as the JSON boundary
/// spells them (the serde `snake_case` names). Each list is the schema's
/// `enum` for the field, the parser's accepted set and the error's
/// "accepted values" — one list, three uses.
pub mod accepted {
    /// `hole_shape` of a pad.
    pub const HOLE_SHAPES: &[&str] = &["round", "square", "slot"];
    /// `paste_mask_expansion_mode` / `solder_mask_expansion_mode`.
    pub const MASK_EXPANSION_MODES: &[&str] = &["none", "manual", "from_rule"];
    /// `power_plane_connect_style` of a pad or via.
    pub const POWER_PLANE_CONNECT_STYLES: &[&str] = &["relief", "direct", "no_connect"];
    /// `stack_mode` of a pad and `diameter_stack_mode` of a via.
    pub const STACK_MODES: &[&str] = &["simple", "top_middle_bottom", "full_stack"];
    /// `drill_layer_pair_type` of a via.
    pub const DRILL_LAYER_PAIR_TYPES: &[&str] = &["through", "blind_buried_start", "mid", "end"];
    /// `kind` of a `PcbLib` text.
    pub const TEXT_KINDS: &[&str] = &["stroke", "true_type", "bar_code"];
    /// `stroke_font` of a `PcbLib` text.
    pub const STROKE_FONTS: &[&str] = &["default", "sans_serif", "serif"];
    /// `kind` of a region, by name (a raw `KIND` integer is accepted too).
    pub const REGION_KINDS: &[&str] = &["copper", "cutout", "named_region", "cavity"];
    /// `justification` of a `PcbLib` text or a `SchLib` label.
    pub const TEXT_JUSTIFICATIONS: &[&str] = &[
        "bottom_left",
        "bottom_center",
        "bottom_right",
        "middle_left",
        "middle_center",
        "middle_right",
        "top_left",
        "top_center",
        "top_right",
    ];
    /// `orientation` of a pin.
    pub const PIN_ORIENTATIONS: &[&str] = &["left", "right", "up", "down"];
    /// `electrical_type` of a pin.
    pub const PIN_ELECTRICAL_TYPES: &[&str] = &[
        "input",
        "output",
        "bidirectional",
        "passive",
        "power",
        "open_collector",
        "open_emitter",
        "hi_z",
    ];
    /// The four `symbol_*` decorations of a pin.
    pub const PIN_SYMBOLS: &[&str] = &[
        "none",
        "dot",
        "right_left_signal_flow",
        "clock",
        "active_low_input",
        "analog_signal_in",
        "not_logic_connection",
        "postponed_output",
        "open_collector",
        "hi_z",
        "high_current",
        "pulse",
        "schmitt",
        "active_low_output",
        "open_collector_pull_up",
        "open_emitter",
        "open_emitter_pull_up",
        "digital_signal_in",
        "shift_left",
        "open_output",
        "left_right_signal_flow",
        "bidirectional_signal_flow",
    ];

    /// Spellings accepted besides the names above, folded (lower case, no
    /// separators) → the name they stand for.
    pub const TEXT_JUSTIFICATION_SYNONYMS: &[(&str, &str)] = &[
        ("bottomcentre", "bottom_center"),
        ("centerleft", "middle_left"),
        ("centreleft", "middle_left"),
        ("middlecentre", "middle_center"),
        ("center", "middle_center"),
        ("centre", "middle_center"),
        ("centerright", "middle_right"),
        ("centreright", "middle_right"),
        ("topcentre", "top_center"),
    ];
    /// Spellings accepted besides [`PIN_ELECTRICAL_TYPES`].
    pub const PIN_ELECTRICAL_TYPE_SYNONYMS: &[(&str, &str)] = &[
        ("io", "bidirectional"),
        ("inputoutput", "bidirectional"),
        ("tristate", "hi_z"),
        ("highimpedance", "hi_z"),
    ];
    /// Spellings accepted besides [`PIN_SYMBOLS`].
    pub const PIN_SYMBOL_SYNONYMS: &[(&str, &str)] = &[
        ("invert", "dot"),
        ("inversion", "dot"),
        ("clk", "clock"),
        ("lowinput", "active_low_input"),
        ("lowoutput", "active_low_output"),
        ("rightleft", "right_left_signal_flow"),
        ("leftright", "left_right_signal_flow"),
        ("bidirectional", "bidirectional_signal_flow"),
        ("analog", "analog_signal_in"),
        ("digital", "digital_signal_in"),
        ("notlogic", "not_logic_connection"),
        ("postponed", "postponed_output"),
        ("highimpedance", "hi_z"),
        ("schmitttrigger", "schmitt"),
    ];
    /// Spellings accepted besides [`HOLE_SHAPES`].
    pub const HOLE_SHAPE_SYNONYMS: &[(&str, &str)] = &[("circle", "round"), ("circular", "round")];
}

/// A spelling without case or the `_`, `-` and space separators, so the
/// serde name, a camel-cased or a spaced form compare equal.
fn fold_spelling(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect()
}

/// Resolves an enum-valued string to the JSON boundary's spelling: one of
/// `accepted` (the serde names) in any case and with or without separators,
/// or a `synonyms` entry. An unrecognised value is an error naming the
/// field and the accepted values — never a silent default.
fn parse_enum<T: serde::de::DeserializeOwned>(
    value: &str,
    field: &str,
    accepted: &[&str],
    synonyms: &[(&str, &str)],
) -> Result<T, String> {
    let folded = fold_spelling(value);
    let name = accepted
        .iter()
        .copied()
        .find(|name| fold_spelling(name) == folded)
        .or_else(|| {
            synonyms
                .iter()
                .find(|(synonym, _)| *synonym == folded)
                .map(|(_, name)| *name)
        })
        .ok_or_else(|| {
            format!(
                "{field} '{value}' is not recognised. Accepted values: {}",
                accepted.join(", ")
            )
        })?;
    serde_json::from_value(Value::String(name.to_string()))
        .map_err(|e| format!("{field} '{value}': {e}"))
}

/// Reads an optional enum-valued string field through [`parse_enum`]: absent
/// is `Ok(None)`; present but not a string, or unrecognised, is an error.
fn enum_field<T: serde::de::DeserializeOwned>(
    json: &Value,
    key: &str,
    field: &str,
    accepted: &[&str],
    synonyms: &[(&str, &str)],
) -> Result<Option<T>, String> {
    match json.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => parse_enum(s, field, accepted, synonyms).map(Some),
        Some(other) => Err(format!(
            "{field} must be a string, got {other}. Accepted values: {}",
            accepted.join(", ")
        )),
    }
}

/// One contour of a region: every vertex must carry both coordinates, since
/// dropping one would silently reshape the outline. `what` names the contour
/// ("outline", "hole 2") in the error.
fn region_vertices(
    points: &[Value],
    what: &str,
) -> Result<Vec<crate::altium::pcblib::Vertex>, String> {
    points
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let coordinate = |key: &str| {
                v.get(key)
                    .and_then(Value::as_f64)
                    .ok_or_else(|| format!("Region {what} vertex {i} is missing a numeric '{key}'"))
            };
            Ok(crate::altium::pcblib::Vertex {
                x: coordinate("x")?,
                y: coordinate("y")?,
            })
        })
        .collect()
}

/// A region's optional interior hole contours: an array of vertex arrays,
/// each a closed contour of at least three whole vertices.
fn region_holes(json: &Value) -> Result<Vec<Vec<crate::altium::pcblib::Vertex>>, String> {
    match json.get("holes") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(contours)) => contours
            .iter()
            .enumerate()
            .map(|(i, contour)| {
                let points = contour
                    .as_array()
                    .ok_or_else(|| format!("Region hole {i} must be an array of vertices"))?;
                let hole = region_vertices(points, &format!("hole {i}"))?;
                if hole.len() < 3 {
                    return Err(format!(
                        "Region hole {i} needs at least 3 vertices, got {}",
                        hole.len()
                    ));
                }
                Ok(hole)
            })
            .collect(),
        Some(other) => Err(format!("Region holes must be an array, got {other}")),
    }
}

/// The spellings every layer-name field accepts; appended to each "invalid
/// layer" error so a caller learns the rule, not just the rejection.
pub const LAYER_NAME_HELP: &str =
    "Valid layer names are Altium's ('Top Overlay', 'Mechanical 13', \
                                   'Mid-Layer 2') or the camel-case form ('TopOverlay', \
                                   'Mechanical13', 'MidLayer2'), in any case.";

pub const PAD_SHAPE_HELP: &str = "Valid shapes are: rectangle (or rectangular), round (or \
     circle), oval, octagonal, rounded_rectangle. Matching is case-insensitive and ignores \
     '_'/'-' separators.";

/// Longest component description the Altium 365 library importer accepts.
/// Altium Designer itself opens a library with a longer one and reads it
/// back whole (measured: 300 characters), so a longer description is written
/// as asked and reported as a validation warning, not refused — the importer
/// names neither the library nor the component when it turns one away, and
/// the warning does.
pub const DESCRIPTION_MAX_LEN: usize = 256;

/// A Windows font face name: at most 31 UTF-16 units, which is all the
/// text record's 64-byte field holds beside its terminator. A longer name
/// would be written cut short without a word.
fn font_face_name<'a>(name: &'a str, field: &str) -> Result<&'a str, String> {
    let units = name.encode_utf16().count();
    if units <= 31 {
        Ok(name)
    } else {
        Err(format!(
            "{field} '{name}' is {units} UTF-16 units long; a Windows font face name has at most 31"
        ))
    }
}

/// A corner-radius percentage: a whole number from 0 to 100. Anything else —
/// negative, fractional, over 100 — is refused, since the writer would
/// otherwise store "no radius" for it without a word.
/// A pad's or via's `polygon_connect` object. Absent keys take AD24's defaults; a
/// value the dialog cannot hold (a count other than 2 or 4, an angle other
/// than 45 or 90, a negative length) is refused rather than written.
fn polygon_connect_field(
    value: &Value,
    field: &str,
) -> Result<crate::altium::pcblib::PolygonConnect, String> {
    use crate::altium::pcblib::PolygonConnect;

    if !value.is_object() {
        return Err(format!("{field} must be an object, got {value}"));
    }
    let defaults = PolygonConnect::default();
    let length = |key: &str, default: f64| match value.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_f64()
            .filter(|mm| mm.is_finite() && *mm >= 0.0)
            .ok_or_else(|| format!("{field}.{key} must be a length in mm (0 or more), got {v}")),
    };
    let conductors = match value.get("conductors") {
        None | Some(Value::Null) => defaults.conductors,
        Some(v) => match v.as_u64() {
            Some(2) => 2,
            Some(4) => 4,
            _ => return Err(format!("{field}.conductors must be 2 or 4, got {v}")),
        },
    };
    let rotation = match value.get("rotation") {
        None | Some(Value::Null) => defaults.rotation,
        Some(v) => match v.as_f64() {
            Some(45.0) => 45,
            Some(90.0) => 90,
            _ => return Err(format!("{field}.rotation must be 45 or 90, got {v}")),
        },
    };
    let flag = |key: &str, default: bool| match value.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_bool()
            .ok_or_else(|| format!("{field}.{key} must be true or false, got {v}")),
    };
    let auto_conductors = flag("auto_conductors", defaults.auto_conductors)?;
    let min_distance_enabled = flag("min_distance_enabled", defaults.min_distance_enabled)?;
    Ok(PolygonConnect {
        style: enum_field(
            value,
            "style",
            &format!("{field}.style"),
            accepted::POWER_PLANE_CONNECT_STYLES,
            &[],
        )?
        .unwrap_or_default(),
        air_gap: length("air_gap", defaults.air_gap)?,
        conductor_width: length("conductor_width", defaults.conductor_width)?,
        conductors,
        auto_conductors,
        rotation,
        min_distance: length("min_distance", defaults.min_distance)?,
        min_distance_enabled,
    })
}

fn percent(value: &Value, field: &str) -> Result<u8, String> {
    value
        .as_u64()
        .and_then(|v| u8::try_from(v).ok())
        .filter(|&v| v <= 100)
        .ok_or_else(|| format!("{field} must be a whole number from 0 to 100, got {value}"))
}

/// The entries of a pad's per-layer array (`key`), held to what the record
/// stores: none on a simple pad; three, `[top, mid, bottom]`, on a
/// top-middle-bottom pad — which keeps sizes and shapes only; thirty-two on
/// a full stack. The writer fills a missing layer from the pad's main value
/// and ignores an extra one, so a count that does not match is refused here
/// rather than mended in silence. `field` names the pad and key in errors.
fn stack_entries<'a>(
    json: &'a Value,
    key: &str,
    stack_mode: crate::altium::pcblib::PadStackMode,
    full_stack_only: bool,
    field: &str,
) -> Result<Option<&'a [Value]>, String> {
    use crate::altium::pcblib::PadStackMode;

    let Some(value) = json.get(key).filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let entries = value
        .as_array()
        .ok_or_else(|| format!("{field} must be an array, got {value}"))?;
    let (expected, mode, layout) = match stack_mode {
        PadStackMode::Simple => {
            return Err(format!(
                "{field} is given but stack_mode is simple; set stack_mode to \
                 top_middle_bottom (3 entries: top, mid, bottom) or full_stack \
                 (32 entries: index 0 = Top, 1 = Bottom, 2-31 = Mid layers)"
            ))
        }
        PadStackMode::TopMiddleBottom if full_stack_only => {
            return Err(format!(
                "{field} applies to a full_stack pad only; a top_middle_bottom pad \
                 stores per-layer sizes and shapes"
            ))
        }
        PadStackMode::TopMiddleBottom => (3, "top_middle_bottom", "[top, mid, bottom]"),
        PadStackMode::FullStack => (
            32,
            "full_stack",
            "index 0 = Top, 1 = Bottom, 2-31 = Mid layers",
        ),
    };
    if entries.len() != expected {
        return Err(format!(
            "{field} has {} entries; {mode} takes {expected} ({layout})",
            entries.len()
        ));
    }
    Ok(Some(entries.as_slice()))
}

impl McpServer {
    /// Parses a pad shape name, shared by `write_pcblib` and `update_pad` so the
    /// same spelling is accepted everywhere.
    ///
    /// Both tools must accept the same spellings, or a pad written with one
    /// cannot be updated with the other. This takes the union of the
    /// vocabularies they document, case-insensitively, ignoring `_`/`-`.
    pub(crate) fn parse_pad_shape(s: &str) -> Option<crate::altium::pcblib::PadShape> {
        use crate::altium::pcblib::PadShape;
        match s.to_lowercase().replace(['_', '-'], "").as_str() {
            "rectangle" | "rectangular" | "rect" => Some(PadShape::Rectangle),
            "round" | "circle" | "circular" => Some(PadShape::Round),
            "oval" | "oblong" => Some(PadShape::Oval),
            "octagonal" | "octagon" => Some(PadShape::Octagonal),
            "roundedrectangle" | "rounded" => Some(PadShape::RoundedRectangle),
            _ => None,
        }
    }

    // ==================== Primitive Parsing Helpers ====================

    /// Parses one symbol object — the `symbols[]` element of `write_schlib`
    /// and the `symbol` of `update_component` — into a [`Symbol`], refusing
    /// unknown keys on every object and validating the geometry. Both tools
    /// go through here so neither can fall behind the other on a record kind
    /// or a replay field. The designator is always assigned: explicit
    /// `designator`, else `designator_prefix`, else `component_type` via the
    /// IEEE 315 / ASME Y14.44 table, else `U`.
    ///
    /// `operation` names the calling tool; `default_name` is the symbol name
    /// when the object carries none.
    #[allow(clippy::too_many_lines)] // one straight-line pass over every symbol field
    #[allow(clippy::unused_self)] // kept as a method beside parse_footprint_json
    pub(crate) fn parse_symbol_json(
        &self,
        sym_json: &Value,
        keys: &allowed_keys::SchLibKeys,
        operation: &str,
        filepath: &str,
        default_name: &str,
    ) -> Result<Symbol, ToolCallResult> {
        use crate::altium::schlib::FootprintModel;

        Self::refuse_unknown(sym_json, &keys.symbol)?;
        let name = sym_json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(default_name);
        let mut symbol = Symbol::new(name);

        if let Some(desc) = sym_json.get("description").and_then(Value::as_str) {
            symbol.description = desc.to_string();
        }

        // Always assign a reference designator. Precedence:
        //   1. explicit `designator`
        //   2. explicit `designator_prefix`
        //   3. `component_type` mapped via IEEE 315 / ASME Y14.44 table
        //   4. fallback "U" (integrated circuit)
        // so every symbol carries a `<prefix>?` designator in the SchLib.
        let designator = sym_json
            .get("designator")
            .and_then(Value::as_str)
            .map_or_else(
                || {
                    let prefix = sym_json
                        .get("designator_prefix")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .or_else(|| {
                            sym_json
                                .get("component_type")
                                .and_then(Value::as_str)
                                .map(|t| ieee_designator_prefix(t).to_string())
                        })
                        .unwrap_or_else(|| "U".to_string());
                    format!("{prefix}?")
                },
                str::to_string,
            );
        symbol.designator = designator;

        // Designator text position (RECORD=34 Location.X/Y) and identity.
        // Defaults -5/5 per the AD24 golden; the unique id is reused when
        // supplied (e.g. a read-modify-write) so the record is deterministic.
        if let Some(x) = sym_json.get("designator_x").and_then(Value::as_f64) {
            symbol.designator_x = x;
        }
        if let Some(y) = sym_json.get("designator_y").and_then(Value::as_f64) {
            symbol.designator_y = y;
        }
        if let Some(uid) = sym_json.get("designator_unique_id").and_then(Value::as_str) {
            symbol.designator_unique_id = Some(uid.to_string());
        }

        // Parse part_count for multi-part symbols (e.g., dual op-amp)
        if let Some(part_count) = sym_json.get("part_count").and_then(Value::as_u64) {
            #[allow(clippy::cast_possible_truncation)]
            {
                symbol.part_count = part_count.clamp(1, 255) as u32;
            }
        }

        // Parse the remaining symbol header fields (mirrors
        // update_schlib_component): export_schlib emits them, so an
        // export -> write_schlib round-trip must not reset them to
        // defaults (e.g. collapsing a two-display-mode symbol to one).
        if let Some(v) = sym_json.get("display_mode_count").and_then(Value::as_u64) {
            symbol.display_mode_count = u32::try_from(v).unwrap_or(symbol.display_mode_count);
        }
        if let Some(v) = sym_json.get("current_part_id").and_then(Value::as_u64) {
            symbol.current_part_id = u32::try_from(v).unwrap_or(symbol.current_part_id);
        }
        if let Some(v) = sym_json.get("part_id_locked").and_then(Value::as_bool) {
            symbol.part_id_locked = v;
        }
        if let Some(v) = sym_json.get("source_library_name").and_then(Value::as_str) {
            symbol.source_library_name = v.to_string();
        }
        if let Some(v) = sym_json.get("target_file_name").and_then(Value::as_str) {
            symbol.target_file_name = v.to_string();
        }

        // Parse pins
        if let Some(pins) = sym_json.get("pins").and_then(Value::as_array) {
            for (i, pin_json) in pins.iter().enumerate() {
                Self::refuse_unknown(pin_json, &keys.pin)?;
                let pin = Self::parse_schlib_pin(pin_json).map_err(|reason| {
                    Self::malformed(operation, filepath, name, "pin", i, &reason)
                })?;
                symbol.add_pin(pin);
            }
        }

        // Parse rectangles
        if let Some(rects) = sym_json.get("rectangles").and_then(Value::as_array) {
            for (i, rect_json) in rects.iter().enumerate() {
                Self::refuse_unknown(rect_json, allowed_keys::RECTANGLE)?;
                let rect = Self::parse_schlib_rectangle(rect_json).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "rectangle",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                symbol.add_rectangle(rect);
            }
        }

        // Parse rounded rectangles
        if let Some(round_rects) = sym_json.get("round_rects").and_then(Value::as_array) {
            for (i, round_rect_json) in round_rects.iter().enumerate() {
                Self::refuse_unknown(round_rect_json, allowed_keys::ROUND_RECT)?;
                let round_rect =
                    Self::parse_schlib_round_rect(round_rect_json).ok_or_else(|| {
                        Self::malformed(
                            operation,
                            filepath,
                            name,
                            "round_rect",
                            i,
                            "a required field is missing or invalid",
                        )
                    })?;
                symbol.add_round_rect(round_rect);
            }
        }

        // Parse lines
        if let Some(lines) = sym_json.get("lines").and_then(Value::as_array) {
            for (i, line_json) in lines.iter().enumerate() {
                Self::refuse_unknown(line_json, allowed_keys::LINE)?;
                let line = Self::parse_schlib_line(line_json).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "line",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                symbol.add_line(line);
            }
        }

        // Parse polylines
        if let Some(polylines) = sym_json.get("polylines").and_then(Value::as_array) {
            for (i, polyline_json) in polylines.iter().enumerate() {
                Self::refuse_unknown(polyline_json, allowed_keys::POLYLINE)?;
                let polyline = Self::parse_schlib_polyline(polyline_json).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "polyline",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                symbol.add_polyline(polyline);
            }
        }

        // Parse polygons
        if let Some(polygons) = sym_json.get("polygons").and_then(Value::as_array) {
            for (i, polygon_json) in polygons.iter().enumerate() {
                Self::refuse_unknown(polygon_json, allowed_keys::POLYGON)?;
                let polygon = Self::parse_schlib_polygon(polygon_json).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "polygon",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                symbol.add_polygon(polygon);
            }
        }

        // Parse arcs
        if let Some(arcs) = sym_json.get("arcs").and_then(Value::as_array) {
            for (i, arc_json) in arcs.iter().enumerate() {
                // SchLib arcs are centre/radius/angle based, NOT layer-based like PcbLib arcs; the
                // allow-list must match the documented fields in tool_definitions or every arc is
                // rejected as an "unknown field" (was erroneously copied from the PcbLib arc as ["layer"]).
                Self::refuse_unknown(arc_json, allowed_keys::ARC)?;
                let arc = Self::parse_schlib_arc(arc_json).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "arc",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                symbol.add_arc(arc);
            }
        }

        if let Some(pies) = sym_json.get("pies").and_then(Value::as_array) {
            for (i, pie_json) in pies.iter().enumerate() {
                Self::refuse_unknown(pie_json, allowed_keys::PIE)?;
                let pie = Self::parse_schlib_pie(pie_json).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "pie",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                symbol.add_pie(pie);
            }
        }

        if let Some(images) = sym_json.get("images").and_then(Value::as_array) {
            for (i, image_json) in images.iter().enumerate() {
                Self::refuse_unknown(image_json, allowed_keys::IMAGE)?;
                let image = Self::parse_schlib_image(image_json).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "image",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                symbol.add_image(image);
            }
        }

        if let Some(text_frames) = sym_json.get("text_frames").and_then(Value::as_array) {
            for (i, frame_json) in text_frames.iter().enumerate() {
                Self::refuse_unknown(frame_json, allowed_keys::TEXT_FRAME)?;
                let text_frame = Self::parse_schlib_text_frame(frame_json).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "text_frame",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                symbol.add_text_frame(text_frame);
            }
        }

        if let Some(beziers) = sym_json.get("beziers").and_then(Value::as_array) {
            for (i, bezier_json) in beziers.iter().enumerate() {
                Self::refuse_unknown(bezier_json, allowed_keys::BEZIER)?;
                let bezier = Self::parse_schlib_bezier(bezier_json).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "bezier",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                symbol.add_bezier(bezier);
            }
        }

        if let Some(ell_arcs) = sym_json.get("elliptical_arcs").and_then(Value::as_array) {
            for (i, ell_arc_json) in ell_arcs.iter().enumerate() {
                Self::refuse_unknown(ell_arc_json, allowed_keys::ELLIPTICAL_ARC)?;
                let ell_arc = Self::parse_schlib_elliptical_arc(ell_arc_json).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "elliptical_arc",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                symbol.add_elliptical_arc(ell_arc);
            }
        }

        // Parse ellipses
        if let Some(ellipses) = sym_json.get("ellipses").and_then(Value::as_array) {
            for (i, ellipse_json) in ellipses.iter().enumerate() {
                Self::refuse_unknown(ellipse_json, allowed_keys::ELLIPSE)?;
                let ellipse = Self::parse_schlib_ellipse(ellipse_json).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "ellipse",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                symbol.add_ellipse(ellipse);
            }
        }

        // Parse labels
        if let Some(labels) = sym_json.get("labels").and_then(Value::as_array) {
            for (i, label_json) in labels.iter().enumerate() {
                Self::refuse_unknown(label_json, allowed_keys::LABEL)?;
                let label = Self::parse_schlib_label(label_json).map_err(|reason| {
                    Self::malformed(operation, filepath, name, "label", i, &reason)
                })?;
                symbol.add_label(label);
            }
        }

        // Parse IEEE symbols
        if let Some(symbols) = sym_json.get("ieee_symbols").and_then(Value::as_array) {
            for (i, symbol_json) in symbols.iter().enumerate() {
                Self::refuse_unknown(symbol_json, allowed_keys::IEEE_SYMBOL)?;
                let ieee_symbol = Self::parse_schlib_ieee_symbol(symbol_json).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "IEEE symbol",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                symbol.add_ieee_symbol(ieee_symbol);
            }
        }

        // Parse parameters
        if let Some(params) = sym_json.get("parameters").and_then(Value::as_array) {
            for (i, param_json) in params.iter().enumerate() {
                Self::refuse_unknown(param_json, allowed_keys::PARAMETER)?;
                let param = Self::parse_schlib_parameter(param_json).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "parameter",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                symbol.add_parameter(param);
            }
        }

        // Parse footprint references
        if let Some(footprints) = sym_json.get("footprints").and_then(Value::as_array) {
            for (i, fp_json) in footprints.iter().enumerate() {
                // A footprint reference is a model link, not an embedded
                // footprint: name, description, library_path, and the
                // read-preserved identity a replay carries back.
                Self::refuse_unknown(fp_json, &keys.footprint)?;
                let fp_name = fp_json.get("name").and_then(Value::as_str).ok_or_else(|| {
                    Self::malformed(
                        operation,
                        filepath,
                        name,
                        "footprint link",
                        i,
                        "a required field is missing or invalid",
                    )
                })?;
                {
                    let mut fp = FootprintModel::new(fp_name);
                    if let Some(desc) = fp_json.get("description").and_then(Value::as_str) {
                        fp.description = desc.to_string();
                    }
                    // Optional PcbLib path -> ModelDatafile0, so Altium can
                    // resolve the footprint instead of reporting "not found".
                    if let Some(lib_path) = fp_json.get("library_path").and_then(Value::as_str) {
                        fp.library_path = Some(lib_path.to_string());
                    }
                    fp.is_current = fp_json
                        .get("is_current")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    fp.unique_id = fp_json
                        .get("unique_id")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    fp.raw_params = json_raw_params(fp_json);
                    symbol.add_footprint(fp);
                }
            }
        }

        // The streams this crate does not read, carried verbatim.
        symbol.extra_streams = json_extra_streams(sym_json);
        // The header exactly as read — key order, unmodelled keys and the
        // stale AllPinCount — so a read-modify-write reproduces it.
        if let Some(params) = sym_json.get("header_params") {
            symbol.header_params = serde_json::from_value(params.clone()).unwrap_or_default();
        }
        if let Some(count) = sym_json.get("all_pin_count").and_then(Value::as_u64) {
            symbol.all_pin_count = u32::try_from(count).ok();
        }

        // The interleaved record order `read_schlib` reported; replaying it
        // replaces the grouped order the add_* calls accumulated, so a
        // read-modify-write keeps the source's record order.
        //
        // With no order to replay the accumulated one is discarded rather than
        // kept: the add_* calls above ran in this function's parse order, which
        // takes pins before rectangles. That is not an authoring order, and
        // emitting it writes a solid-filled body *after* the pins it encloses,
        // where it paints over their names. Clearing hands the sequence back to
        // `SchPrimitiveKind::WRITE_ORDER`, which leads with the body graphics
        // for exactly this reason.
        // The storage the symbol was read from, so a read-modify-write
        // through JSON keeps it where Altium looks for it (#507).
        symbol.storage_name = sym_json
            .get("storage_name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        match sym_json.get("primitive_order") {
            Some(order) => match serde_json::from_value(order.clone()) {
                Ok(kinds) => symbol.primitive_order = kinds,
                Err(e) => {
                    tracing::debug!(error = %e, "invalid primitive_order; using default order");
                    symbol.primitive_order.clear();
                }
            },
            None => symbol.primitive_order.clear(),
        }

        // Out-of-range or non-finite geometry is refused here so both tools
        // report it the same way.
        // Text the record format cannot hold is refused here, by field,
        // rather than by the writer after a backup has already been made.
        if let Err(e) = symbol.check_record_text() {
            return Err(ToolCallResult::error(e));
        }
        if let Err(e) = Self::validate_symbol_coordinates(&symbol) {
            return Err(ToolCallResult::error(e));
        }

        Ok(symbol)
    }

    /// Refuses a JSON object carrying a key outside `keys` (see
    /// [`check_unknown_fields`](Self::check_unknown_fields)).
    fn refuse_unknown(json: &Value, keys: &[&str]) -> Result<(), ToolCallResult> {
        Self::check_unknown_fields(json, keys).map_err(ToolCallResult::error)
    }

    /// The error for an object its parser could not build: a required field
    /// missing or of the wrong type. Named and indexed like a malformed pad,
    /// so a bad record is refused rather than silently left out of the file.
    fn malformed(
        operation: &str,
        filepath: &str,
        component: &str,
        kind: &str,
        index: usize,
        reason: &str,
    ) -> ToolCallResult {
        ToolCallResult::error_with_context(
            ErrorContext::new(operation, format!("Malformed {kind}: {reason}"))
                .with_filepath(filepath)
                .with_component(component)
                .with_details(format!("Failed to parse {kind} at index {index}")),
        )
    }

    /// Parses one footprint object — the `footprints[]` element of
    /// `write_pcblib` and the `footprint` of `update_component` — into a
    /// [`Footprint`], refusing unknown keys on every object and validating the
    /// geometry. Both tools go through here so neither can fall behind the
    /// other on a primitive kind, a 3D-model spelling or a replay field.
    ///
    /// `operation` names the calling tool in error context; `default_name` is
    /// the footprint name when the object carries none.
    #[allow(clippy::too_many_lines)] // one straight-line pass over every footprint field
    pub(crate) fn parse_footprint_json(
        &self,
        fp_json: &Value,
        keys: &allowed_keys::PcbLibKeys,
        operation: &str,
        filepath: &str,
        default_name: &str,
    ) -> Result<Footprint, ToolCallResult> {
        use crate::altium::pcblib::Model3D;

        Self::refuse_unknown(fp_json, &keys.footprint)?;
        let name = fp_json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(default_name);
        let mut footprint = Footprint::new(name);

        if let Some(desc) = fp_json.get("description").and_then(Value::as_str) {
            footprint.description = desc.to_string();
        }
        if let Some(height) = fp_json.get("height") {
            let Some(height) = height
                .as_f64()
                .filter(|height| height.is_finite() && *height >= 0.0)
            else {
                return Err(ToolCallResult::error(format!(
                    "Footprint '{name}' height must be a finite number of millimetres, zero or more"
                )));
            };
            footprint.height = height;
        }

        // Parse pads
        if let Some(pads) = fp_json.get("pads").and_then(Value::as_array) {
            for (i, pad_json) in pads.iter().enumerate() {
                Self::refuse_unknown(pad_json, &keys.pad)?;
                match Self::parse_pad(pad_json) {
                    Ok(pad) => footprint.add_pad(pad),
                    Err(e) => {
                        return Err(ToolCallResult::error_with_context(
                            ErrorContext::new(operation, e)
                                .with_filepath(filepath)
                                .with_component(name)
                                .with_details(format!("Failed to parse pad at index {i}")),
                        ))
                    }
                }
            }
        }

        // Parse tracks
        if let Some(tracks) = fp_json.get("tracks").and_then(Value::as_array) {
            for (i, track_json) in tracks.iter().enumerate() {
                Self::refuse_unknown(track_json, &keys.track)?;
                match Self::parse_track(track_json) {
                    Ok(track) => footprint.add_track(track),
                    Err(e) => {
                        return Err(ToolCallResult::error_with_context(
                            ErrorContext::new(operation, e)
                                .with_filepath(filepath)
                                .with_component(name)
                                .with_details(format!("Failed to parse track at index {i}")),
                        ))
                    }
                }
            }
        }

        // Parse vias
        if let Some(vias) = fp_json.get("vias").and_then(Value::as_array) {
            for (i, via_json) in vias.iter().enumerate() {
                Self::refuse_unknown(via_json, &keys.via)?;
                match Self::parse_via(via_json) {
                    Ok(via) => footprint.add_via(via),
                    Err(e) => {
                        return Err(ToolCallResult::error_with_context(
                            ErrorContext::new(operation, e)
                                .with_filepath(filepath)
                                .with_component(name)
                                .with_details(format!("Failed to parse via at index {i}")),
                        ))
                    }
                }
            }
        }

        // Parse fills
        if let Some(fills) = fp_json.get("fills").and_then(Value::as_array) {
            for (i, fill_json) in fills.iter().enumerate() {
                Self::refuse_unknown(fill_json, &keys.fill)?;
                match Self::parse_fill(fill_json) {
                    Ok(fill) => footprint.add_fill(fill),
                    Err(e) => {
                        return Err(ToolCallResult::error_with_context(
                            ErrorContext::new(operation, e)
                                .with_filepath(filepath)
                                .with_component(name)
                                .with_details(format!("Failed to parse fill at index {i}")),
                        ))
                    }
                }
            }
        }

        // Parse arcs
        if let Some(arcs) = fp_json.get("arcs").and_then(Value::as_array) {
            for (i, arc_json) in arcs.iter().enumerate() {
                Self::refuse_unknown(arc_json, &keys.arc)?;
                match Self::parse_arc(arc_json) {
                    Ok(arc) => footprint.add_arc(arc),
                    Err(e) => {
                        return Err(ToolCallResult::error_with_context(
                            ErrorContext::new(operation, e)
                                .with_filepath(filepath)
                                .with_component(name)
                                .with_details(format!("Failed to parse arc at index {i}")),
                        ))
                    }
                }
            }
        }

        // Parse regions
        if let Some(regions) = fp_json.get("regions").and_then(Value::as_array) {
            for (i, region_json) in regions.iter().enumerate() {
                Self::refuse_unknown(region_json, &keys.region)?;
                let region = Self::parse_region(region_json).map_err(|reason| {
                    Self::malformed(operation, filepath, name, "region", i, &reason)
                })?;
                footprint.add_region(region);
            }
        }

        // Parse text
        if let Some(texts) = fp_json.get("text").and_then(Value::as_array) {
            for (i, text_json) in texts.iter().enumerate() {
                Self::refuse_unknown(text_json, &keys.text)?;
                let text = Self::parse_text(text_json).map_err(|reason| {
                    Self::malformed(operation, filepath, name, "text", i, &reason)
                })?;
                footprint.add_text(text);
            }
        }

        // Parse 3D model
        if let Some(model_json) = fp_json.get("step_model") {
            Self::refuse_unknown(model_json, &keys.model)?;
            if let Some(model_path) = model_json.get("filepath").and_then(Value::as_str) {
                let embed = model_json
                    .get("embed")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);

                if embed {
                    // The embed source is read from disk at save time
                    // (prepare_3d_models_for_writing -> std::fs::read), far from
                    // this handler. Validate it against the allow-list now so a
                    // caller cannot embed an arbitrary file (e.g. "../../etc/passwd")
                    // into the library. External references (embed=false) are only
                    // stored as a string and never read, so they are not gated here.
                    if let Err(e) = self.validate_path(model_path) {
                        return Err(ToolCallResult::error(e));
                    }

                    // Embedded model - use Model3D which will read the file on save
                    footprint.model_3d = Some(Model3D {
                        filepath: model_path.to_string(),
                        x_offset: model_json
                            .get("x_offset")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                        y_offset: model_json
                            .get("y_offset")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                        z_offset: model_json
                            .get("z_offset")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                        rotation: model_json
                            .get("rotation")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                    });
                } else {
                    // External reference only: a model-backed body whose file
                    // stays outside the library. Altium stores such a body with
                    // an EMPTY MODELID and the MODEL.* group carrying
                    // MODEL.EMBED=FALSE and MODEL.NAME (a UI-authored
                    // `test_0805.step` reference), and the writer emits the
                    // group for any body that names a file. The full path is
                    // kept so organised subfolders resolve.
                    use crate::altium::pcblib::{ComponentBody, Layer};
                    footprint.add_component_body(ComponentBody {
                        model_id: String::new(),
                        identifier: String::new(),
                        texture_center_x: None,
                        texture_center_y: None,
                        texture_size_x: None,
                        texture_size_y: None,
                        texture_rotation: None,
                        raw_layer_id: None,
                        v7_layer: None,
                        model_name: model_path.to_string(), // Preserve full path
                        embedded: false,
                        rotation_x: 0.0,
                        rotation_y: 0.0,
                        rotation_z: model_json
                            .get("rotation")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                        z_offset: model_json
                            .get("z_offset")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                        overall_height: 0.0,
                        standoff_height: 0.0,
                        cavity_height: 0.0,
                        layer: Layer::Top3DBody,
                        outline: Vec::new(),
                        unique_id: None,
                        guid: None,
                        model_checksum: 0, // External reference: no embedded model.
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
                        // External reference: no board association (free primitive).
                        net_index: 0xFFFF,
                        polygon_index: 0xFFFF,
                        component_index: -1,
                        additional_parameters: Vec::new(),
                        param_key_order: Vec::new(),
                    });
                }
            }
        }

        // Parse "model_3d" — read_pcblib's spelling of the same model
        // reference (it emits the key for every footprint, null when there
        // is no model), accepted so a read result replays into
        // write_pcblib unchanged. `step_model` wins when both are given
        // (it is the authoring-time spelling, incl. the embed switch);
        // null is ignored. The fields mirror the Model3D serde shape
        // (filepath + offsets/rotation).
        if fp_json.get("step_model").is_none() {
            if let Some(model_json) = fp_json.get("model_3d").filter(|v| !v.is_null()) {
                Self::refuse_unknown(model_json, &keys.model)?;
                let model_path = model_json
                    .get("filepath")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                // The save path embeds the file (std::fs::read) when the
                // path resolves to an existing file, so gate exactly that
                // case against the allow-list — the same arbitrary-file-
                // read defence as step_model. Bare model names replayed
                // from read_pcblib output don't exist on disk and are kept
                // as inert references, so they are not gated.
                if std::path::Path::new(model_path).is_file() {
                    if let Err(e) = self.validate_path(model_path) {
                        return Err(ToolCallResult::error(e));
                    }
                }
                footprint.model_3d = Some(Model3D {
                    filepath: model_path.to_string(),
                    x_offset: model_json
                        .get("x_offset")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0),
                    y_offset: model_json
                        .get("y_offset")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0),
                    z_offset: model_json
                        .get("z_offset")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0),
                    rotation: model_json
                        .get("rotation")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0),
                });
            }
        }

        // Parse generic extruded 3D bodies (no STEP model). Each body is
        // defined by an optional 2D outline (auto-bounding-box from pads when
        // omitted) plus standoff/overall heights, on the Top/Bottom 3D Body
        // layer. model_id/model_name stay empty so the writer marks them as
        // shape-based extruded bodies.
        if let Some(bodies) = fp_json.get("component_bodies").and_then(Value::as_array) {
            for (i, body_json) in bodies.iter().enumerate() {
                Self::refuse_unknown(body_json, &keys.component_body)?;
                match Self::parse_component_body_json(body_json) {
                    Ok(body) => footprint.add_component_body(body),
                    Err(e) => {
                        return Err(ToolCallResult::error_with_context(
                            ErrorContext::new(operation, e)
                                .with_filepath(filepath)
                                .with_component(name)
                                .with_details(format!(
                                    "Failed to parse component body at index {i}"
                                )),
                        ))
                    }
                }
            }
        }

        // Footprint-level replay fields `read_pcblib` emits. The guid is
        // the kind-85 identity record; the interleaved Data-stream order
        // replaces the grouped order the add_* calls accumulated, so a
        // read-modify-write keeps the source's stream order
        // (`write_sequence` is advisory-safe against a stale list).
        footprint.guid = match guid_field(fp_json, "guid", &format!("Footprint '{name}' guid")) {
            Ok(guid) => guid,
            Err(e) => return Err(ToolCallResult::error(e)),
        };
        // The storage the footprint was read from, so a read-modify-write
        // through JSON keeps it where Altium looks for it (#507).
        footprint.storage_name = fp_json
            .get("storage_name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        if let Some(order) = fp_json.get("primitive_order") {
            match serde_json::from_value(order.clone()) {
                Ok(kinds) => footprint.primitive_order = kinds,
                Err(e) => {
                    tracing::debug!(error = %e, "invalid primitive_order; using default order");
                }
            }
        }
        // The Parameters block as read, so a read-modify-write through JSON
        // writes back every key Altium put there.
        footprint.additional_parameters = Self::parse_additional_parameters(fp_json);
        footprint.param_key_order = Self::parse_key_order(fp_json);

        // Out-of-range or non-finite geometry would saturate in from_mm() on
        // save; refused here so both tools report it the same way.
        // Text the record format cannot hold is refused here, by field,
        // rather than by the writer after a backup has already been made.
        if let Err(e) = footprint.check_record_text() {
            return Err(ToolCallResult::error(e));
        }
        if let Err(e) = Self::validate_footprint_coordinates(&footprint) {
            return Err(ToolCallResult::error(e));
        }

        Ok(footprint)
    }

    pub(crate) fn check_unknown_fields(
        json: &serde_json::Value,
        allowed_keys: &[&str],
    ) -> Result<(), String> {
        if let Some(obj) = json.as_object() {
            for key in obj.keys() {
                if !allowed_keys.contains(&key.as_str()) {
                    return Err(format!(
                        "Unknown field '{key}'. Allowed fields are: {allowed_keys:?}"
                    ));
                }
            }
        }
        Ok(())
    }

    /// Parses a pad from JSON.
    #[allow(clippy::too_many_lines)] // Pad has many fields requiring individual parsing
    pub(crate) fn parse_pad(json: &Value) -> Result<crate::altium::pcblib::Pad, String> {
        use crate::altium::pcblib::{
            Layer, MaskExpansionMode, Pad, PadStackMode, PowerPlaneConnectStyle,
        };

        let designator = json
            .get("designator")
            .and_then(Value::as_str)
            .ok_or("Pad missing required field 'designator'")?;

        // Validate designator is not empty
        if designator.trim().is_empty() {
            return Err("Pad designator cannot be empty".to_string());
        }

        let x = json
            .get("x")
            .and_then(Value::as_f64)
            .ok_or("Pad missing required field 'x'")?;
        let y = json
            .get("y")
            .and_then(Value::as_f64)
            .ok_or("Pad missing required field 'y'")?;
        let width = json
            .get("width")
            .and_then(Value::as_f64)
            .ok_or("Pad missing required field 'width'")?;
        let height = json
            .get("height")
            .and_then(Value::as_f64)
            .ok_or("Pad missing required field 'height'")?;

        // Validate pad dimensions are positive
        if width <= 0.0 {
            return Err(format!(
                "Pad '{designator}' has invalid width {width}. Width must be greater than 0."
            ));
        }
        if height <= 0.0 {
            return Err(format!(
                "Pad '{designator}' has invalid height {height}. Height must be greater than 0."
            ));
        }

        // Omitting `shape` yields RoundedRectangle. That suits chip/QFN lands but is
        // wrong for BGA/CSP, whose circular NSMD lands need an explicit "round" —
        // see the `shape` description in tool_definitions.rs.
        let shape_str = json
            .get("shape")
            .and_then(Value::as_str)
            .unwrap_or("rounded_rectangle");
        let shape = Self::parse_pad_shape(shape_str).ok_or_else(|| {
            format!("Pad '{designator}' has invalid shape '{shape_str}'. {PAD_SHAPE_HELP}")
        })?;

        // Parse hole_size first to determine default layer
        let hole_size = json.get("hole_size").and_then(Value::as_f64);
        let is_smd = hole_size.is_none_or(|h| h <= 0.0); // SMD if no hole or hole size <= 0

        // Plated hole (main-block byte @60). Altium defaults this to true for
        // every pad, SMD included (matches the golden fixture and AltiumSharp).
        let is_plated = json
            .get("is_plated")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        let layer_str = json.get("layer").and_then(Value::as_str);
        let layer = match layer_str {
            Some(s) => Layer::parse(s).ok_or_else(|| {
                format!("Pad '{designator}' has invalid layer '{s}'. {LAYER_NAME_HELP}")
            })?,
            // SMD pads default to Top Layer, through-hole pads default to Multi-Layer
            None => {
                if is_smd {
                    Layer::TopLayer
                } else {
                    Layer::MultiLayer
                }
            }
        };
        let rotation = json.get("rotation").and_then(Value::as_f64).unwrap_or(0.0);

        // Enum-valued fields: absent keeps the from-scratch default, an
        // unrecognised value is refused rather than silently defaulted.
        let pad_field = |key: &str| format!("Pad '{designator}' {key}");
        let hole_shape = enum_field(
            json,
            "hole_shape",
            &pad_field("hole_shape"),
            accepted::HOLE_SHAPES,
            accepted::HOLE_SHAPE_SYNONYMS,
        )?
        .unwrap_or_default();

        // Parse optional mask expansion values
        let paste_mask_expansion = json.get("paste_mask_expansion").and_then(Value::as_f64);
        let solder_mask_expansion = json.get("solder_mask_expansion").and_then(Value::as_f64);
        let paste_mask_expansion_mode: MaskExpansionMode = enum_field(
            json,
            "paste_mask_expansion_mode",
            &pad_field("paste_mask_expansion_mode"),
            accepted::MASK_EXPANSION_MODES,
            &[],
        )?
        .unwrap_or_default();
        let solder_mask_expansion_mode: MaskExpansionMode = enum_field(
            json,
            "solder_mask_expansion_mode",
            &pad_field("solder_mask_expansion_mode"),
            accepted::MASK_EXPANSION_MODES,
            &[],
        )?
        .unwrap_or_default();

        // The corner radius is a percentage; out of range is refused rather
        // than quietly read as "none".
        let corner_radius_percent = match json.get("corner_radius_percent") {
            None | Some(Value::Null) => None,
            Some(value) => Some(percent(value, &pad_field("corner_radius_percent"))?),
        };

        // Thermal-relief / power-plane connection fields. Absent keys keep the
        // from-scratch defaults (= Altium's pad template), so an unspecified pad
        // round-trips byte-identically.
        let power_plane_connect_style: PowerPlaneConnectStyle = enum_field(
            json,
            "power_plane_connect_style",
            &pad_field("power_plane_connect_style"),
            accepted::POWER_PLANE_CONNECT_STYLES,
            &[],
        )?
        .unwrap_or_default();
        let relief_conductor_width = json
            .get("relief_conductor_width")
            .and_then(Value::as_f64)
            .unwrap_or(0.254);
        let relief_entries = json
            .get("relief_entries")
            .and_then(Value::as_i64)
            .and_then(|v| i16::try_from(v).ok())
            .unwrap_or(4);
        let relief_air_gap = json
            .get("relief_air_gap")
            .and_then(Value::as_f64)
            .unwrap_or(0.254);
        let power_plane_relief_expansion = json
            .get("power_plane_relief_expansion")
            .and_then(Value::as_f64)
            .unwrap_or(0.508);
        let power_plane_clearance = json
            .get("power_plane_clearance")
            .and_then(Value::as_f64)
            .unwrap_or(0.508);
        let polygon_connect = match json.get("polygon_connect") {
            None | Some(Value::Null) => None,
            Some(value) => Some(polygon_connect_field(value, &pad_field("polygon_connect"))?),
        };

        // Slot geometry + drill tolerances. Absent keys keep the struct defaults
        // (slot 0, rotation 0, tolerances unset), so an unspecified pad round-trips
        // byte-identically.
        let hole_slot_length = json
            .get("hole_slot_length")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let hole_rotation = json
            .get("hole_rotation")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let hole_positive_tolerance = json.get("hole_positive_tolerance").and_then(Value::as_f64);
        let hole_negative_tolerance = json.get("hole_negative_tolerance").and_then(Value::as_f64);

        // Identity GUIDs (extended tail @126/@142). Absent -> None, so the
        // writer generates fresh per-pad GUIDs; a read-modify-write passes the
        // read value back and preserves the on-disk bytes verbatim.
        let identity_guid = guid_field(json, "identity_guid", &pad_field("identity_guid"))?;
        let identity_guid_b = guid_field(json, "identity_guid_b", &pad_field("identity_guid_b"))?;

        let solder_mask_expansion_from_hole_edge = json
            .get("solder_mask_expansion_from_hole_edge")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Per-layer pad stack. The model has carried these all along; without them
        // in the schema a caller could never author anything but a Simple pad.
        let stack_mode: PadStackMode = enum_field(
            json,
            "stack_mode",
            &pad_field("stack_mode"),
            accepted::STACK_MODES,
            &[],
        )?
        .unwrap_or_default();
        // Every per-layer array is held to what the record stores (see
        // `stack_entries`), and an entry that is not what it must be is
        // refused by index — never read as a zero pair, a round layer or no
        // radius. Pairs take any spelling the tools accept (`{width, height}`,
        // `{x, y}` or `[a, b]`).
        let entries = |key: &str, full_stack_only: bool| {
            stack_entries(json, key, stack_mode, full_stack_only, &pad_field(key))
        };
        let pairs = |key: &str, full_stack_only: bool, spelling: &str| {
            entries(key, full_stack_only)?
                .map(|entries| {
                    entries
                        .iter()
                        .enumerate()
                        .map(|(i, v)| {
                            crate::altium::serde_round::pair_from_json(v).ok_or_else(|| {
                                format!("{}[{i}] must be {spelling}, got {v}", pad_field(key))
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()
                })
                .transpose()
        };
        let per_layer_sizes = pairs(
            "per_layer_sizes",
            false,
            "{width, height} (or [width, height])",
        )?;
        let per_layer_offsets = pairs("per_layer_offsets", true, "{x, y} (or [x, y])")?;
        let per_layer_shapes = entries("per_layer_shapes", false)?
            .map(|entries| {
                entries
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let field = format!("{}[{i}]", pad_field("per_layer_shapes"));
                        let name = v.as_str().ok_or_else(|| {
                            format!("{field} must be a string, got {v}. {PAD_SHAPE_HELP}")
                        })?;
                        let shape = Self::parse_pad_shape(name).ok_or_else(|| {
                            format!("{field} '{name}' is not a shape. {PAD_SHAPE_HELP}")
                        })?;
                        // The rounding lives in the per-layer corner-radius
                        // bytes, which only a full stack's block stores — a
                        // top_middle_bottom slot would come back plain round.
                        if shape == crate::altium::pcblib::PadShape::RoundedRectangle
                            && stack_mode == crate::altium::pcblib::PadStackMode::TopMiddleBottom
                        {
                            return Err(format!(
                                "{field}: rounded_rectangle needs a per-layer corner \
                                 radius, which only a full_stack pad stores; a \
                                 top_middle_bottom stack cannot hold it"
                            ));
                        }
                        Ok(shape)
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .transpose()?;
        let per_layer_corner_radii = entries("per_layer_corner_radii", true)?
            .map(|entries| {
                entries
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        percent(v, &format!("{}[{i}]", pad_field("per_layer_corner_radii")))
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .transpose()?;

        Ok(Pad {
            raw_layer_id: json_raw_layer_id(json),
            designator: designator.to_string(),
            x,
            y,
            width,
            height,
            shape,
            layer,
            hole_size,
            is_plated,
            jumper_id: i16::try_from(json.get("jumper_id").and_then(Value::as_i64).unwrap_or(0))
                .unwrap_or(0),
            solder_mask_expansion_from_hole_edge,
            hole_shape,
            hole_slot_length,
            hole_rotation,
            hole_positive_tolerance,
            hole_negative_tolerance,
            rotation,
            paste_mask_expansion,
            solder_mask_expansion,
            paste_mask_expansion_mode,
            solder_mask_expansion_mode,
            power_plane_connect_style,
            relief_conductor_width,
            relief_entries,
            relief_air_gap,
            power_plane_relief_expansion,
            power_plane_clearance,
            polygon_connect,
            corner_radius_percent,
            stack_mode,
            per_layer_sizes,
            per_layer_shapes,
            per_layer_corner_radii,
            per_layer_offsets,
            net_index: json_net_index(json),
            polygon_index: json_polygon_index(json),
            component_index: json_component_index(json),
            flags: json_flags(json),
            unique_id: json_unique_id(json),
            guid: guid_field(json, "guid", &pad_field("guid"))?,
            raw_tail: json_base64(json, "raw_tail"),
            identity_guid,
            identity_guid_b,
        })
    }

    /// Parses a track from JSON.
    pub(crate) fn parse_track(json: &Value) -> Result<crate::altium::pcblib::Track, String> {
        use crate::altium::pcblib::{Layer, Track};

        let x1 = json
            .get("x1")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'x1'")?;
        let y1 = json
            .get("y1")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'y1'")?;
        let x2 = json
            .get("x2")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'x2'")?;
        let y2 = json
            .get("y2")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'y2'")?;
        let width = json
            .get("width")
            .and_then(Value::as_f64)
            .ok_or("Track missing required field 'width'")?;

        let layer_str = json.get("layer").and_then(Value::as_str);
        let layer = match layer_str {
            Some(s) => Layer::parse(s)
                .ok_or_else(|| format!("Track has invalid layer '{s}'. {LAYER_NAME_HELP}"))?,
            None => Layer::TopOverlay, // Default for tracks is Top Overlay
        };

        let mut track = Track::new(x1, y1, x2, y2, width, layer);
        // Optional EE tail (mirrors the modelled optionals; absent keys keep the
        // `Track::new` defaults so a from-scratch track is byte-identical).
        track.flags = json_flags(json);
        track.net_index = json_net_index(json);
        track.polygon_index = json_polygon_index(json);
        track.component_index = json_component_index(json);
        track.solder_mask_expansion = json_f64(json, "solder_mask_expansion");
        track.keepout_restrictions = json_keepout(json);
        track.unique_id = json_unique_id(json);
        track.guid = guid_field(json, "guid", "Track guid")?;
        track.raw_layer_id = json_raw_layer_id(json);
        Ok(track)
    }

    /// Parses an arc from JSON.
    pub(crate) fn parse_arc(json: &Value) -> Result<crate::altium::pcblib::Arc, String> {
        use crate::altium::pcblib::{Arc, Layer};

        let x = json
            .get("x")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'x'")?;
        let y = json
            .get("y")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'y'")?;
        let radius = json
            .get("radius")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'radius'")?;
        let start_angle = json
            .get("start_angle")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'start_angle'")?;
        let end_angle = json
            .get("end_angle")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'end_angle'")?;
        let width = json
            .get("width")
            .and_then(Value::as_f64)
            .ok_or("Arc missing required field 'width'")?;

        let layer_str = json.get("layer").and_then(Value::as_str);
        let layer = match layer_str {
            Some(s) => Layer::parse(s)
                .ok_or_else(|| format!("Arc has invalid layer '{s}'. {LAYER_NAME_HELP}"))?,
            None => Layer::TopOverlay, // Default for arcs is Top Overlay
        };

        Ok(Arc {
            raw_layer_id: json_raw_layer_id(json),
            x,
            y,
            radius,
            start_angle,
            end_angle,
            width,
            layer,
            flags: json_flags(json),
            net_index: json_net_index(json),
            polygon_index: json_polygon_index(json),
            component_index: json_component_index(json),
            unique_id: json_unique_id(json),
            guid: guid_field(json, "guid", "Arc guid")?,
            // Optional EE tail (mirrors the modelled optionals; absent keys keep
            // the default `None` so a from-scratch arc is byte-identical).
            solder_mask_expansion: json_f64(json, "solder_mask_expansion"),
            keepout_restrictions: json_keepout(json),
        })
    }

    /// Parses a region from JSON.
    pub(crate) fn parse_region(json: &Value) -> Result<crate::altium::pcblib::Region, String> {
        use crate::altium::pcblib::{Layer, Region, RegionKind};

        let vertices_json = json
            .get("vertices")
            .and_then(Value::as_array)
            .ok_or("Region is missing its vertices array")?;
        let layer = match json.get("layer").and_then(Value::as_str) {
            Some(s) => Layer::parse(s)
                .ok_or_else(|| format!("Region has invalid layer '{s}'. {LAYER_NAME_HELP}"))?,
            None => Layer::Mechanical15,
        };

        let vertices = region_vertices(vertices_json, "outline")?;
        if vertices.len() < 3 {
            return Err(format!(
                "Region needs at least 3 vertices, got {}",
                vertices.len()
            ));
        }

        // `kind` accepts a name (matching the serde representation) or a raw
        // KIND integer. Board cutouts are not a kind of their own — AD24 stores
        // one as copper on the keep-out layer with `ISBOARDCUTOUT=TRUE`.
        let kind = match json.get("kind") {
            None | Some(Value::Null) => RegionKind::Copper,
            Some(Value::String(s)) => match s.trim().parse::<i32>() {
                Ok(id) => RegionKind::from_id(id),
                Err(_) => parse_enum(s, "Region kind", accepted::REGION_KINDS, &[])?,
            },
            Some(Value::Number(n)) => n
                .as_i64()
                .and_then(|i| i32::try_from(i).ok())
                .map(RegionKind::from_id)
                .ok_or_else(|| format!("Region kind {n} is not a KIND integer"))?,
            // The serde form of a kind outside the named ones (`{"other": n}`),
            // as read_pcblib echoes it.
            Some(other) => serde_json::from_value::<RegionKind>(other.clone()).map_err(|_| {
                format!(
                    "Region kind {other} is not recognised. Accepted values: {}, or a KIND integer",
                    accepted::REGION_KINDS.join(", ")
                )
            })?,
        };
        let name = json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let net_index = json_net_index(json);
        let polygon_index = json_polygon_index(json);
        let component_index = json_component_index(json);
        let cavity_height = json
            .get("cavity_height")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        // These four are always serialised by read_pcblib (no skip_serializing_if),
        // so a read-modify-write must accept AND preserve them, not reset to default.
        // Their defaults mirror Region::default() so a from-scratch region is unchanged.
        let arc_resolution = json
            .get("arc_resolution")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let sub_poly_index = json_i32(json, "sub_poly_index").unwrap_or(-1);
        let union_index = json_i32(json, "union_index").unwrap_or(0);
        let is_shape_based = json
            .get("is_shape_based")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let holes = region_holes(json)?;

        let text_field = |key: &str| json.get(key).and_then(Value::as_str).map(str::to_string);

        // Round-trip unmodelled board-region keys captured on read (a `read_pcblib`
        // emits `additional_parameters` as an array of `[key, value]` pairs; accept
        // that verbatim so a modify-write preserves them). Absent -> empty -> the
        // writer appends nothing (byte-identical to a from-scratch region).
        let additional_parameters = Self::parse_additional_parameters(json);

        Ok(Region {
            vertices,
            holes,
            layer,
            // Derived from `layer` unless a read supplied a divergent token
            // (a board cutout); the tool schema does not expose it.
            v7_layer: text_field("v7_layer"),
            flags: json_flags(json),
            kind,
            name,
            net_index,
            polygon_index,
            component_index,
            arc_resolution,
            cavity_height,
            sub_poly_index,
            union_index,
            is_shape_based,
            unique_id: text_field("unique_id"),
            guid: guid_field(json, "guid", "Region guid")?,
            additional_parameters,
            param_key_order: Self::parse_key_order(json),
        })
    }

    /// Parses the `param_key_order` list a `read_pcblib` region emits, so a
    /// tool-level read-modify-write keeps the block's original key order.
    /// Absent -> empty -> the writer's canonical order.
    pub(crate) fn parse_key_order(json: &Value) -> Vec<String> {
        json.get("param_key_order")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Parses an `additional_parameters` catch-all from a primitive's JSON: an
    /// array of `[key, value]` string pairs (the shape `read_pcblib` emits for the
    /// `Vec<(String, String)>` field). Missing/malformed entries yield an empty
    /// vector, so a from-scratch primitive re-emits nothing.
    pub(crate) fn parse_additional_parameters(json: &Value) -> Vec<(String, String)> {
        json.get("additional_parameters")
            .and_then(Value::as_array)
            .map(|pairs| {
                pairs
                    .iter()
                    .filter_map(|pair| {
                        let arr = pair.as_array()?;
                        let key = arr.first()?.as_str()?;
                        let value = arr.get(1)?.as_str()?;
                        Some((key.to_string(), value.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Parses text from JSON.
    // A flat JSON-to-field mapping: long because Text carries a lot of optional
    // properties, not because it branches.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn parse_text(json: &Value) -> Result<crate::altium::pcblib::Text, String> {
        use crate::altium::pcblib::{Layer, StrokeFont, Text, TextJustification, TextKind};

        let number = |key: &str| {
            json.get(key)
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("Text is missing a numeric '{key}'"))
        };
        let x = number("x")?;
        let y = number("y")?;
        let text = json
            .get("text")
            .and_then(Value::as_str)
            .ok_or("Text is missing its 'text' string")?;
        let height = number("height")?;
        let layer = json
            .get("layer")
            .and_then(Value::as_str)
            .and_then(Layer::parse)
            .unwrap_or(Layer::TopOverlay);
        let rotation = json.get("rotation").and_then(Value::as_f64).unwrap_or(0.0);
        // Optional stroke line width in mm; `None` keeps Altium's template default.
        let stroke_width = json
            .get("stroke_width")
            .and_then(Value::as_f64)
            .filter(|&w| w > 0.0);

        // Style/font fields are now authored from JSON instead of being hard-coded.
        // The string enums (`kind`, `stroke_font`, `justification`) deserialise via
        // serde so the accepted tokens match exactly what `read_pcblib` emits; an
        // absent or unparseable value falls back to the from-scratch default (which
        // keeps a default text byte-identical to the template).
        let kind: TextKind =
            enum_field(json, "kind", "Text kind", accepted::TEXT_KINDS, &[])?.unwrap_or_default();
        let stroke_font: Option<StrokeFont> = enum_field(
            json,
            "stroke_font",
            "Text stroke_font",
            accepted::STROKE_FONTS,
            &[],
        )?;
        let italic = json.get("italic").and_then(Value::as_bool).unwrap_or(false);
        let bold = json.get("bold").and_then(Value::as_bool).unwrap_or(false);
        let mirror = json.get("mirror").and_then(Value::as_bool).unwrap_or(false);
        // Comment/Designator field markers (geometry @40/@41). Absent -> false,
        // the template bytes, so an unspecified text stays byte-identical.
        let is_comment = json
            .get("is_comment")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let is_designator = json
            .get("is_designator")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let font_name = match json.get("font_name").and_then(Value::as_str) {
            None => "Arial".to_string(),
            Some(name) => font_face_name(name, "Text font_name")?.to_string(),
        };
        // The from-scratch default is `BottomLeft` (encodes to the template's
        // 0x03 byte, keeping a default text byte-identical).
        let justification: TextJustification = enum_field(
            json,
            "justification",
            "Text justification",
            accepted::TEXT_JUSTIFICATIONS,
            accepted::TEXT_JUSTIFICATION_SYNONYMS,
        )?
        .unwrap_or(TextJustification::BottomLeft);

        // Inverted (knockout) text-box descriptor. Absent booleans default to
        // false and absent dimensions to `None`, keeping a default text
        // byte-identical to the template.
        let is_inverted = json
            .get("is_inverted")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let inverted_border = json.get("inverted_border").and_then(Value::as_f64);
        let use_inverted_rectangle = json
            .get("use_inverted_rectangle")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let inverted_rect_width = json.get("inverted_rect_width").and_then(Value::as_f64);
        let inverted_rect_height = json.get("inverted_rect_height").and_then(Value::as_f64);
        let inverted_rect_text_offset = json
            .get("inverted_rect_text_offset")
            .and_then(Value::as_f64);

        Ok(Text {
            raw_layer_id: json_raw_layer_id(json),
            x,
            y,
            text: text.to_string(),
            height,
            layer,
            rotation,
            kind,
            stroke_font,
            stroke_width,
            italic,
            bold,
            mirror,
            is_comment,
            is_designator,
            font_name,
            justification,
            is_inverted,
            inverted_border,
            use_inverted_rectangle,
            inverted_rect_width,
            inverted_rect_height,
            inverted_rect_text_offset,
            flags: json_flags(json),
            net_index: json_net_index(json),
            polygon_index: json_polygon_index(json),
            component_index: json_component_index(json),
            unique_id: json_unique_id(json),
            guid: guid_field(json, "guid", "Text guid")?,
            raw_geometry: json_base64(json, "raw_geometry"),
            barcode_full_width: json_f64(json, "barcode_full_width"),
            barcode_full_height: json_f64(json, "barcode_full_height"),
            barcode_x_margin: json_f64(json, "barcode_x_margin"),
            barcode_y_margin: json_f64(json, "barcode_y_margin"),
            barcode_kind: u8::try_from(
                json.get("barcode_kind")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            )
            .unwrap_or(0),
            barcode_font_name: match json.get("barcode_font_name").and_then(Value::as_str) {
                None => String::new(),
                Some(name) => font_face_name(name, "Text barcode_font_name")?.to_string(),
            },
            barcode_inverted: json
                .get("barcode_inverted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            barcode_show_text: json
                .get("barcode_show_text")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    /// Parses a via from JSON.
    ///
    /// Mirrors [`Self::parse_pad`]'s layer-name parsing for the `from_layer` /
    /// Parses a `ComponentBody` (3D body) from JSON. Shared by the write-tool
    /// create path (`call_write_pcblib`) and the in-place update path
    /// (`update_pcblib_component`) so neither can silently drop bodies or drift.
    /// Every field defaults to the create handler's own value, so a
    /// from-scratch body stays byte-identical (oracle 0).
    #[allow(clippy::too_many_lines)] // ComponentBody has many optional fields
    pub(crate) fn parse_component_body_json(
        body_json: &Value,
    ) -> Result<crate::altium::pcblib::ComponentBody, String> {
        use crate::altium::pcblib::{ComponentBody, Layer};

        let layer = match body_json.get("layer").and_then(Value::as_str) {
            None => Layer::Top3DBody,
            Some(s) => Layer::parse(s).ok_or_else(|| {
                format!("Component body has invalid layer '{s}'. {LAYER_NAME_HELP}")
            })?,
        };
        // Vertices in any spelling the tools accept (`{x, y}` or `[x, y]`).
        let outline = body_json
            .get("outline")
            .and_then(Value::as_array)
            .map(|verts| {
                verts
                    .iter()
                    .filter_map(crate::altium::serde_round::pair_from_json)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let f = |k: &str| body_json.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        let str_or = |k: &str, d: &str| {
            body_json
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or(d)
                .to_string()
        };
        Ok(ComponentBody {
            identifier: str_or("identifier", ""),
            texture_center_x: json_guidless_opt(body_json, "texture_center_x"),
            texture_center_y: json_guidless_opt(body_json, "texture_center_y"),
            texture_size_x: json_guidless_opt(body_json, "texture_size_x"),
            texture_size_y: json_guidless_opt(body_json, "texture_size_y"),
            texture_rotation: json_guidless_opt(body_json, "texture_rotation"),
            raw_layer_id: json_raw_layer_id(body_json),
            v7_layer: json_guidless_opt(body_json, "v7_layer"),
            model_id: str_or("model_id", ""),
            model_name: str_or("model_name", ""),
            embedded: body_json
                .get("embedded")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            rotation_x: f("rotation_x"),
            rotation_y: f("rotation_y"),
            rotation_z: f("rotation_z"),
            z_offset: f("z_offset"),
            overall_height: f("overall_height"),
            standoff_height: f("standoff_height"),
            cavity_height: f("cavity_height"),
            layer,
            outline,
            unique_id: body_json
                .get("unique_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            guid: guid_field(body_json, "guid", "Component body guid")?,
            model_checksum: body_json
                .get("model_checksum")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            name: str_or("name", " "),
            kind: body_json
                .get("kind")
                .and_then(Value::as_u64)
                .and_then(|v| u8::try_from(v).ok())
                .unwrap_or(0),
            sub_poly_index: body_json
                .get("sub_poly_index")
                .and_then(Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .unwrap_or(-1),
            union_index: body_json
                .get("union_index")
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0),
            is_shape_based: body_json
                .get("is_shape_based")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            body_projection: body_json
                .get("body_projection")
                .and_then(Value::as_u64)
                .and_then(|v| u8::try_from(v).ok())
                .unwrap_or(0),
            body_color_3d: body_json
                .get("body_color_3d")
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(8_421_504),
            body_opacity_3d: body_json
                .get("body_opacity_3d")
                .and_then(Value::as_f64)
                .unwrap_or(1.0),
            model_2d_rotation: body_json
                .get("model_2d_rotation")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            model_2d_x: body_json
                .get("model_2d_x")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            model_2d_y: body_json
                .get("model_2d_y")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            net_index: body_json
                .get("net_index")
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .unwrap_or(0xFFFF),
            polygon_index: body_json
                .get("polygon_index")
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .unwrap_or(0xFFFF),
            component_index: body_json
                .get("component_index")
                .and_then(Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .unwrap_or(-1),
            additional_parameters: Self::parse_additional_parameters(body_json),
            param_key_order: Self::parse_key_order(body_json),
        })
    }

    /// `to_layer` fields and reuses [`crate::altium::pcblib::MaskExpansionMode`]
    /// string parsing for the mask mode. Optionals default exactly as
    /// [`crate::altium::pcblib::Via::new`] does when absent.
    #[allow(clippy::too_many_lines)] // Via has many optional fields requiring individual parsing
    pub(crate) fn parse_via(json: &Value) -> Result<crate::altium::pcblib::Via, String> {
        use crate::altium::pcblib::{
            DrillLayerPairType, Layer, MaskExpansionMode, PowerPlaneConnectStyle, Via,
        };

        let x = json
            .get("x")
            .and_then(Value::as_f64)
            .ok_or("Via missing required field 'x'")?;
        let y = json
            .get("y")
            .and_then(Value::as_f64)
            .ok_or("Via missing required field 'y'")?;
        let diameter = json
            .get("diameter")
            .and_then(Value::as_f64)
            .ok_or("Via missing required field 'diameter'")?;
        let hole_size = json
            .get("hole_size")
            .and_then(Value::as_f64)
            .ok_or("Via missing required field 'hole_size'")?;

        // Validate via dimensions are sensible: the hole must fit inside the
        // annular ring, both positive.
        if diameter <= 0.0 {
            return Err(format!(
                "Via has invalid diameter {diameter}. Diameter must be greater than 0."
            ));
        }
        if hole_size <= 0.0 {
            return Err(format!(
                "Via has invalid hole_size {hole_size}. Hole size must be greater than 0."
            ));
        }
        if hole_size >= diameter {
            return Err(format!(
                "Via hole_size {hole_size} must be smaller than diameter {diameter}."
            ));
        }

        // Start from the struct's defaults (top->bottom layers, standard thermal
        // relief), then override with any supplied fields.
        let mut via = Via::new(x, y, diameter, hole_size);

        if let Some(s) = json.get("from_layer").and_then(Value::as_str) {
            via.from_layer = Layer::parse(s)
                .ok_or_else(|| format!("Via has invalid from_layer '{s}'. {LAYER_NAME_HELP}"))?;
        }
        if let Some(s) = json.get("to_layer").and_then(Value::as_str) {
            via.to_layer = Layer::parse(s)
                .ok_or_else(|| format!("Via has invalid to_layer '{s}'. {LAYER_NAME_HELP}"))?;
        }

        if let Some(v) = json.get("solder_mask_expansion").and_then(Value::as_f64) {
            via.solder_mask_expansion = v;
        }
        if let Some(b) = json
            .get("solder_mask_expansion_from_hole_edge")
            .and_then(Value::as_bool)
        {
            via.solder_mask_expansion_from_hole_edge = b;
        }
        if let Some(kind) = enum_field::<DrillLayerPairType>(
            json,
            "drill_layer_pair_type",
            "Via drill_layer_pair_type",
            accepted::DRILL_LAYER_PAIR_TYPES,
            &[],
        )? {
            via.drill_layer_pair_type = kind;
        }
        if let Some(mode) = enum_field::<MaskExpansionMode>(
            json,
            "solder_mask_expansion_mode",
            "Via solder_mask_expansion_mode",
            accepted::MASK_EXPANSION_MODES,
            &[],
        )? {
            via.solder_mask_expansion_mode = mode;
        }
        if let Some(v) = json.get("thermal_relief_gap").and_then(Value::as_f64) {
            via.thermal_relief_gap = v;
        }
        if let Some(v) = json
            .get("thermal_relief_conductors")
            .and_then(Value::as_u64)
            .and_then(|v| u8::try_from(v).ok())
        {
            via.thermal_relief_conductors = v;
        }
        if let Some(v) = json.get("thermal_relief_width").and_then(Value::as_f64) {
            via.thermal_relief_width = v;
        }

        // Power-plane connection (SubRecord-1 @31/@42/@46) + paste-mask @50 +
        // net index @3. Absent keys keep the from-scratch defaults (= Altium's
        // via template), so an unspecified via round-trips byte-identically.
        if let Some(style) = enum_field::<PowerPlaneConnectStyle>(
            json,
            "power_plane_connect_style",
            "Via power_plane_connect_style",
            accepted::POWER_PLANE_CONNECT_STYLES,
            &[],
        )? {
            via.power_plane_connect_style = style;
        }
        if let Some(v) = json
            .get("power_plane_relief_expansion")
            .and_then(Value::as_f64)
        {
            via.power_plane_relief_expansion = v;
        }
        if let Some(v) = json.get("power_plane_clearance").and_then(Value::as_f64) {
            via.power_plane_clearance = v;
        }
        if let Some(v) = json.get("paste_mask_expansion").and_then(Value::as_f64) {
            via.paste_mask_expansion = v;
        }
        if let Some(v) = json
            .get("net_index")
            .and_then(Value::as_u64)
            .and_then(|v| u16::try_from(v).ok())
        {
            via.net_index = v;
        }
        // Polygon @5 / component @7 connectivity indices. Absent keys keep the
        // from-scratch defaults (none / free primitive), byte-identical.
        via.polygon_index = json_polygon_index(json);
        via.component_index = json_component_index(json);

        // Drill tolerances (SubRecord-1 @291/@295). Absent keys keep the
        // from-scratch defaults (tolerances unset), so an unspecified via
        // round-trips byte-identically.
        if let Some(v) = json.get("hole_positive_tolerance").and_then(Value::as_f64) {
            via.hole_positive_tolerance = Some(v);
        }
        if let Some(v) = json.get("hole_negative_tolerance").and_then(Value::as_f64) {
            via.hole_negative_tolerance = Some(v);
        }

        if let Some(v) = json
            .get("solder_mask_expansion_back")
            .and_then(Value::as_f64)
        {
            via.solder_mask_expansion_back = Some(v);
        }
        via.diameter_stack_mode = enum_field(
            json,
            "diameter_stack_mode",
            "Via diameter_stack_mode",
            accepted::STACK_MODES,
            &[],
        )?
        .unwrap_or_default();
        // The diameter stack, held to the record's 32 slots: the writer would
        // fill a missing layer from `diameter` and ignore an extra one without
        // a word, and a simple via has no stack to fill.
        if let Some(value) = json.get("per_layer_diameters").filter(|v| !v.is_null()) {
            let entries = value
                .as_array()
                .ok_or_else(|| format!("Via per_layer_diameters must be an array, got {value}"))?;
            if via.diameter_stack_mode == crate::altium::pcblib::ViaStackMode::Simple {
                return Err(
                    "Via per_layer_diameters is given but diameter_stack_mode is simple; \
                            set it to top_middle_bottom or full_stack (32 entries: index 0 = Top, \
                            1 = Bottom, 2-31 = Mid layers)"
                        .to_string(),
                );
            }
            if entries.len() != 32 {
                return Err(format!(
                    "Via per_layer_diameters has {} entries; a stacked via takes 32 (index 0 = \
                     Top, 1 = Bottom, 2-31 = Mid layers)",
                    entries.len()
                ));
            }
            let diameters = entries
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    v.as_f64().ok_or_else(|| {
                        format!("Via per_layer_diameters[{i}] must be a number, got {v}")
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            via.per_layer_diameters = Some(diameters);
        }

        via.flags = json_flags(json);
        via.unique_id = json_unique_id(json);
        via.guid = guid_field(json, "guid", "Via guid")?;
        via.raw_block = json_base64(json, "raw_block");
        via.polygon_connect = match json.get("polygon_connect") {
            None | Some(Value::Null) => None,
            Some(value) => Some(polygon_connect_field(value, "Via polygon_connect")?),
        };

        Ok(via)
    }

    /// Parses a fill from JSON.
    ///
    /// Reuses [`Self::parse_pad`]'s layer-name parsing for `layer`. Geometry
    /// (`x1`/`y1`/`x2`/`y2`) is required; `rotation` and the mask/keepout
    /// optionals default as [`crate::altium::pcblib::Fill::new`] does when absent.
    pub(crate) fn parse_fill(json: &Value) -> Result<crate::altium::pcblib::Fill, String> {
        use crate::altium::pcblib::{Fill, Layer};

        let x1 = json
            .get("x1")
            .and_then(Value::as_f64)
            .ok_or("Fill missing required field 'x1'")?;
        let y1 = json
            .get("y1")
            .and_then(Value::as_f64)
            .ok_or("Fill missing required field 'y1'")?;
        let x2 = json
            .get("x2")
            .and_then(Value::as_f64)
            .ok_or("Fill missing required field 'x2'")?;
        let y2 = json
            .get("y2")
            .and_then(Value::as_f64)
            .ok_or("Fill missing required field 'y2'")?;

        let layer_str = json.get("layer").and_then(Value::as_str);
        let layer = match layer_str {
            Some(s) => Layer::parse(s)
                .ok_or_else(|| format!("Fill has invalid layer '{s}'. {LAYER_NAME_HELP}"))?,
            None => Layer::TopLayer, // Default for fills is Top Layer
        };

        let mut fill = Fill::new(x1, y1, x2, y2, layer);

        if let Some(r) = json.get("rotation").and_then(Value::as_f64) {
            fill.rotation = r;
        }
        // Optional flags + mask/keepout tail (mirrors the modelled optionals).
        fill.flags = json_flags(json);
        fill.net_index = json_net_index(json);
        fill.polygon_index = json_polygon_index(json);
        fill.component_index = json_component_index(json);
        fill.solder_mask_expansion = json.get("solder_mask_expansion").and_then(Value::as_f64);
        fill.keepout_restrictions = json_keepout(json);
        fill.unique_id = json_unique_id(json);
        fill.guid = guid_field(json, "guid", "Fill guid")?;
        fill.raw_layer_id = json_raw_layer_id(json);

        Ok(fill)
    }

    // ==================== SchLib Primitive Parsing Helpers ====================

    /// Parses a schematic pin from JSON.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::too_many_lines)] // Pin parsing with symbol attributes requires many lines
    pub(crate) fn parse_schlib_pin(json: &Value) -> Result<crate::altium::schlib::Pin, String> {
        use crate::altium::schlib::{Pin, PinElectricalType, PinOrientation, PinSymbol};

        let designator = json
            .get("designator")
            .and_then(Value::as_str)
            .ok_or("Pin is missing its 'designator' string")?;
        let name = json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(designator);
        let integer = |key: &str| {
            json_i32(json, key)
                .ok_or_else(|| format!("Pin '{designator}' is missing an integer '{key}'"))
        };
        let x = integer("x")?;
        let y = integer("y")?;
        let length = json_i32(json, "length").unwrap_or(10);
        let pin_field = |key: &str| format!("Pin '{designator}' {key}");

        let orientation: PinOrientation = enum_field(
            json,
            "orientation",
            &pin_field("orientation"),
            accepted::PIN_ORIENTATIONS,
            &[],
        )?
        .unwrap_or(PinOrientation::Right);

        let electrical_type: PinElectricalType = enum_field(
            json,
            "electrical_type",
            &pin_field("electrical_type"),
            accepted::PIN_ELECTRICAL_TYPES,
            accepted::PIN_ELECTRICAL_TYPE_SYNONYMS,
        )?
        .unwrap_or(PinElectricalType::Passive);

        let hidden = json.get("hidden").and_then(Value::as_bool).unwrap_or(false);
        let show_name = json
            .get("show_name")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let show_designator = json
            .get("show_designator")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        // The four decorations, each by its serde name or a synonym.
        let pin_symbol = |key: &str| -> Result<PinSymbol, String> {
            Ok(enum_field(
                json,
                key,
                &pin_field(key),
                accepted::PIN_SYMBOLS,
                accepted::PIN_SYMBOL_SYNONYMS,
            )?
            .unwrap_or(PinSymbol::None))
        };
        let symbol_inner_edge = pin_symbol("symbol_inner_edge")?;
        let symbol_outer_edge = pin_symbol("symbol_outer_edge")?;
        let symbol_inside = pin_symbol("symbol_inside")?;
        let symbol_outside = pin_symbol("symbol_outside")?;

        // Authoring fields read from JSON so an
        // AI can set them, matching the names `read_schlib` exposes (serialised
        // straight from the `Pin` struct). `colour` is a BGR integer; absent keys
        // keep the from-scratch defaults (`part_and_sequence` defaults to "|&|").
        let description = json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let colour = json.get("colour").and_then(Value::as_u64).unwrap_or(0) as u32;
        let graphically_locked = json
            .get("graphically_locked")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let swap_id_group = json
            .get("swap_id_group")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let part_and_sequence = json
            .get("part_and_sequence")
            .and_then(Value::as_str)
            .unwrap_or("|&|")
            .to_string();
        let default_value = json
            .get("default_value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // Pin binary-record display mode (own byte, distinct from the shape flag).
        let owner_part_display_mode = json_i32(json, "owner_part_display_mode").unwrap_or(0);
        // Symbol line width; 0 (default) writes no PinSymbolLineWidth aux stream.
        let symbol_line_width = json_i32(json, "symbol_line_width").unwrap_or(0);
        // Fractional pin coordinates ({x,y,length} in 1/100000 DXP units); an
        // all-zero or absent object writes no PinFrac aux stream.
        let frac = json.get("frac").and_then(|f| {
            let pf = crate::altium::schlib::PinFrac {
                x: json_i32(f, "x").unwrap_or(0),
                y: json_i32(f, "y").unwrap_or(0),
                length: json_i32(f, "length").unwrap_or(0),
            };
            (!pf.is_zero()).then_some(pf)
        });
        // Both fields are always serialised by read_schlib (no skip_serializing_if),
        // so a read-modify-write round-trip must accept and preserve them rather than
        // reset them to a hard-coded default. `is_not_accessible` defaults false (the
        // pin conglomerate `0x20` bit); `formal_type` defaults 1 (Altium's normal pin).
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let formal_type = json
            .get("formal_type")
            .and_then(Value::as_u64)
            .and_then(|v| u8::try_from(v).ok())
            .unwrap_or(1);

        Ok(Pin {
            name: name.to_string(),
            designator: designator.to_string(),
            x,
            y,
            length,
            orientation,
            electrical_type,
            hidden,
            show_name,
            show_designator,
            description,
            owner_part_id,
            owner_part_display_mode,
            colour,
            graphically_locked,
            symbol_inner_edge,
            symbol_outer_edge,
            symbol_inside,
            symbol_outside,
            is_not_accessible,
            formal_type,
            swap_id_group,
            part_and_sequence,
            default_value,
            symbol_line_width,
            frac,
        })
    }

    /// Parses a schematic rectangle from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_rectangle(json: &Value) -> Option<crate::altium::schlib::Rectangle> {
        use crate::altium::schlib::Rectangle;

        let x1 = json_f64(json, "x1")?;
        let y1 = json_f64(json, "y1")?;
        let x2 = json_f64(json, "x2")?;
        let y2 = json_f64(json, "y2")?;

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json
            .get("line_color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let fill_color = json
            .get("fill_color")
            .and_then(Value::as_u64)
            .unwrap_or(0xB0_FF_FF) as u32;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        // Style fields read from JSON (matches the
        // names `read_schlib` exposes). `line_style`: 0=Solid, 1=Dashed, 2=Dotted.
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        let transparent = json
            .get("transparent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Rectangle {
            x1,
            y1,
            x2,
            y2,
            line_width,
            line_color,
            fill_color,
            line_style,
            filled,
            transparent,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
            raw_params: json_raw_params(json),
        })
    }

    /// Parses a schematic rounded rectangle from JSON.
    ///
    /// Mirrors [`Self::parse_schlib_rectangle`] (geometry + fill/border colours +
    /// `filled`), adding the `corner_x_radius` / `corner_y_radius` rounding fields.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::similar_names)] // corner_x_radius / corner_y_radius mirror the struct fields
    pub(crate) fn parse_schlib_round_rect(
        json: &Value,
    ) -> Option<crate::altium::schlib::RoundRect> {
        use crate::altium::schlib::RoundRect;

        let x1 = json_f64(json, "x1")?;
        let y1 = json_f64(json, "y1")?;
        let x2 = json_f64(json, "x2")?;
        let y2 = json_f64(json, "y2")?;
        let corner_x_radius = json_f64(json, "corner_x_radius").unwrap_or(0.0);
        let corner_y_radius = json_f64(json, "corner_y_radius").unwrap_or(0.0);

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json
            .get("line_color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let fill_color = json
            .get("fill_color")
            .and_then(Value::as_u64)
            .unwrap_or(0xB0_FF_FF) as u32;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        // Style fields read from JSON (matches the
        // names `read_schlib` exposes). `line_style`: 0=Solid, 1=Dashed, 2=Dotted.
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        let transparent = json
            .get("transparent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(RoundRect {
            x1,
            y1,
            x2,
            y2,
            corner_x_radius,
            corner_y_radius,
            line_width,
            line_color,
            fill_color,
            line_style,
            filled,
            transparent,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
            raw_params: json_raw_params(json),
        })
    }

    /// Parses a schematic line from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_line(json: &Value) -> Option<crate::altium::schlib::Line> {
        use crate::altium::schlib::Line;

        // Coordinates accept fractional values; integer-only `json_i32` would drop
        // an off-grid endpoint like 3.75.
        let x1 = json_f64(json, "x1")?;
        let y1 = json_f64(json, "y1")?;
        let x2 = json_f64(json, "x2")?;
        let y2 = json_f64(json, "y2")?;

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        // `line_style` read from JSON (matches the name
        // `read_schlib` exposes). 0=Solid, 1=Dashed, 2=Dotted.
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        // `is_not_accessible` read from JSON, defaulting true (matches
        // the name `read_schlib` exposes). Altium tags every line, so default true.
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Line {
            x1,
            y1,
            x2,
            y2,
            line_width,
            color,
            line_style,
            is_not_accessible,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
            raw_params: json_raw_params(json),
        })
    }

    /// Parses a schematic parameter from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_parameter(json: &Value) -> Option<crate::altium::schlib::Parameter> {
        use crate::altium::schlib::Parameter;

        let name = json.get("name").and_then(Value::as_str)?;
        let value = json
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("*")
            .to_string();

        let x = json_f64(json, "x").unwrap_or(0.0);
        let y = json_f64(json, "y").unwrap_or(0.0);
        let font_id = json.get("font_id").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x80_00_00) as u32;
        let hidden = json.get("hidden").and_then(Value::as_bool).unwrap_or(false);
        // De-hardcoded: the core already models these, so read them from JSON.
        // Defaults equal the previous hard-coded values, keeping a default
        // parameter byte-identical.
        let read_only_state = json
            .get("read_only_state")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8;
        let param_type = json.get("param_type").and_then(Value::as_u64).unwrap_or(0) as u8;
        let unique_id = json
            .get("unique_id")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        // EE-meaningful display fields (omit-when-default).
        let orientation = json_i32(json, "orientation").unwrap_or(0);
        // Altium anchor id 0-8 (0 = bottom-left ... 8 = top-right); default 0.
        let justification = json
            .get("justification")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8;
        let show_name = json
            .get("show_name")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let hide_name = json
            .get("hide_name")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let description = json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let is_configurable = json
            .get("is_configurable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let auto_position = json
            .get("auto_position")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let is_rule = json
            .get("is_rule")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let is_system_parameter = json
            .get("is_system_parameter")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let text_horz_anchor = json
            .get("text_horz_anchor")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8;
        let text_vert_anchor = json
            .get("text_vert_anchor")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8;
        let is_mirrored = json
            .get("is_mirrored")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Parameter {
            raw_params: json_raw_params(json),
            name: name.to_string(),
            value,
            x,
            y,
            font_id,
            color,
            hidden,
            read_only_state,
            param_type,
            orientation,
            justification,
            is_mirrored,
            show_name,
            hide_name,
            description,
            is_configurable,
            auto_position,
            is_rule,
            is_system_parameter,
            text_horz_anchor,
            text_vert_anchor,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id,
        })
    }

    /// Parses a schematic polyline from JSON.
    /// Accepts both "points" and "vertices" field names for the coordinate array.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_polyline(json: &Value) -> Option<crate::altium::schlib::Polyline> {
        use crate::altium::schlib::Polyline;

        // Accept both "points" and "vertices" for flexibility
        let points_json = json
            .get("points")
            .or_else(|| json.get("vertices"))
            .and_then(Value::as_array)?;
        // Each point in any spelling the tools accept (`{x, y}` or `[x, y]`).
        let points: Vec<(f64, f64)> = points_json
            .iter()
            .filter_map(crate::altium::serde_round::pair_from_json)
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .collect();

        if points.len() < 2 {
            return None; // Need at least 2 points for a polyline
        }

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        // Style + arrowhead fields read from JSON
        // (matches the names `read_schlib` exposes). `line_style`: 0=Solid,
        // 1=Dashed, 2=Dotted. `start_line_shape`/`end_line_shape` are endpoint
        // (arrowhead) shapes and `line_shape_size` their size.
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        let start_line_shape = json
            .get("start_line_shape")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8;
        let end_line_shape = json
            .get("end_line_shape")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8;
        let line_shape_size = json
            .get("line_shape_size")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8;
        let transparent = json
            .get("transparent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        // `is_not_accessible` read from JSON (matches
        // the name `read_schlib` exposes). Altium tags every polyline, so
        // default true.
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Polyline {
            points,
            line_width,
            color,
            line_style,
            start_line_shape,
            end_line_shape,
            line_shape_size,
            transparent,
            is_not_accessible,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
            raw_params: json_raw_params(json),
        })
    }

    /// Parses a schematic filled polygon from JSON.
    ///
    /// Mirrors [`Self::parse_schlib_polyline`] (reads the `points`/`vertices`
    /// array of `[x, y]` pairs), adding the polygon's `filled` / `fill_color`
    /// fields.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_polygon(json: &Value) -> Option<crate::altium::schlib::Polygon> {
        use crate::altium::schlib::Polygon;

        // Accept both "points" and "vertices" for flexibility (matches polyline).
        let points_json = json
            .get("points")
            .or_else(|| json.get("vertices"))
            .and_then(Value::as_array)?;
        // Each point in any spelling the tools accept (`{x, y}` or `[x, y]`).
        let points: Vec<(f64, f64)> = points_json
            .iter()
            .filter_map(crate::altium::serde_round::pair_from_json)
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .collect();

        if points.len() < 3 {
            return None; // Need at least 3 vertices for a polygon
        }

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json
            .get("line_color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let fill_color = json
            .get("fill_color")
            .and_then(Value::as_u64)
            .unwrap_or(0xB0_FF_FF) as u32;
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        let transparent = json
            .get("transparent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Polygon {
            points,
            line_width,
            line_color,
            fill_color,
            line_style,
            filled,
            transparent,
            is_not_accessible,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
            raw_params: json_raw_params(json),
        })
    }

    /// Parses a schematic arc from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_arc(json: &Value) -> Option<crate::altium::schlib::Arc> {
        use crate::altium::schlib::Arc;

        let x = json_f64(json, "x")?;
        let y = json_f64(json, "y")?;
        let radius = json_f64(json, "radius")?;
        let start_angle = json
            .get("start_angle")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let end_angle = json
            .get("end_angle")
            .and_then(Value::as_f64)
            .unwrap_or(360.0);
        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        // `fill_color` read from JSON, defaulting 0 (matches the name
        // `read_schlib` exposes). Maps to the `AreaColor` param; 0 = no fill.
        let fill_color = json.get("fill_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        // `is_not_accessible` read from JSON, defaulting true (matches
        // the name `read_schlib` exposes). Altium tags every arc, so default true.
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Arc {
            x,
            y,
            radius,
            is_not_accessible,
            start_angle,
            end_angle,
            line_width,
            color,
            fill_color,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
            raw_params: json_raw_params(json),
        })
    }

    /// Parses a schematic pie (filled circular sector, `RECORD=9`) from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_pie(json: &Value) -> Option<crate::altium::schlib::Pie> {
        use crate::altium::schlib::Pie;

        let x = json_f64(json, "x")?;
        let y = json_f64(json, "y")?;
        let radius = json_f64(json, "radius")?;
        let start_angle = json
            .get("start_angle")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let end_angle = json
            .get("end_angle")
            .and_then(Value::as_f64)
            .unwrap_or(360.0);
        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json.get("line_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let fill_color = json.get("fill_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        let transparent = json
            .get("transparent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Pie {
            x,
            y,
            radius,
            is_not_accessible,
            start_angle,
            end_angle,
            line_width,
            line_color,
            fill_color,
            filled,
            transparent,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
            raw_params: json_raw_params(json),
        })
    }

    /// Parses a schematic image (embedded/linked picture, `RECORD=30`) from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_image(json: &Value) -> Option<crate::altium::schlib::Image> {
        use crate::altium::schlib::Image;

        let x1 = json_f64(json, "x1")?;
        let y1 = json_f64(json, "y1")?;
        let x2 = json_f64(json, "x2")?;
        let y2 = json_f64(json, "y2")?;
        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json.get("line_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        let fill_color = json.get("fill_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let b = |k: &str| json.get(k).and_then(Value::as_bool).unwrap_or(false);
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let file_name = json
            .get("file_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // Base64-encoded raw image bytes destined for the library /Storage
        // stream.
        let image_data = json_base64(json, "image_data");
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Image {
            x1,
            y1,
            x2,
            y2,
            is_not_accessible,
            line_width,
            line_color,
            line_style,
            fill_color,
            filled: b("filled"),
            transparent: b("transparent"),
            show_border: b("show_border"),
            keep_aspect: b("keep_aspect"),
            embed_image: b("embed_image"),
            file_name,
            image_data,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
            raw_params: json_raw_params(json),
        })
    }

    /// Parses a schematic text frame (bordered multi-line text box,
    /// `RECORD=28`) from JSON. Requires the frame box (`x1`..`y2`) and `text`;
    /// optionals default as [`crate::altium::schlib::TextFrame::new`] does when
    /// absent (white fill, centre alignment, border/word-wrap/clip-to-rect on).
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_text_frame(
        json: &Value,
    ) -> Option<crate::altium::schlib::TextFrame> {
        use crate::altium::schlib::TextFrame;

        let x1 = json_f64(json, "x1")?;
        let y1 = json_f64(json, "y1")?;
        let x2 = json_f64(json, "x2")?;
        let y2 = json_f64(json, "y2")?;
        let text = json.get("text").and_then(Value::as_str)?.to_string();
        let color = json.get("color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let area_color = json
            .get("area_color")
            .and_then(Value::as_u64)
            .unwrap_or(16_777_215) as u32;
        let text_color = json.get("text_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let text_margin = json
            .get("text_margin")
            .and_then(Value::as_f64)
            .unwrap_or(0.000_05);
        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(0) as u8;
        let line_style = json.get("line_style").and_then(Value::as_u64).unwrap_or(0) as u8;
        let font_id = json.get("font_id").and_then(Value::as_u64).unwrap_or(1) as u8;
        let orientation = json.get("orientation").and_then(Value::as_u64).unwrap_or(0) as u8;
        let alignment = json.get("alignment").and_then(Value::as_u64).unwrap_or(1) as u8;
        let b_false = |k: &str| json.get(k).and_then(Value::as_bool).unwrap_or(false);
        let b_true = |k: &str| json.get(k).and_then(Value::as_bool).unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(TextFrame {
            x1,
            y1,
            x2,
            y2,
            text,
            color,
            area_color,
            text_color,
            text_margin,
            line_width,
            line_style,
            transparent: b_false("transparent"),
            font_id,
            orientation,
            alignment,
            is_solid: b_false("is_solid"),
            show_border: b_true("show_border"),
            word_wrap: b_true("word_wrap"),
            clip_to_rect: b_true("clip_to_rect"),
            is_not_accessible: b_true("is_not_accessible"),
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
            raw_params: json_raw_params(json),
        })
    }

    /// Parses a `SchLib` Bezier from JSON. Requires the four control points
    /// (`x1`..`y4`); optionals default as [`crate::altium::schlib::Bezier::new`]
    /// does when absent.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_bezier(json: &Value) -> Option<crate::altium::schlib::Bezier> {
        use crate::altium::schlib::Bezier;

        let x1 = json_f64(json, "x1")?;
        let y1 = json_f64(json, "y1")?;
        let x2 = json_f64(json, "x2")?;
        let y2 = json_f64(json, "y2")?;
        let x3 = json_f64(json, "x3")?;
        let y3 = json_f64(json, "y3")?;
        let x4 = json_f64(json, "x4")?;
        let y4 = json_f64(json, "y4")?;
        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Bezier {
            x1,
            y1,
            x2,
            y2,
            x3,
            y3,
            x4,
            y4,
            line_width,
            color,
            is_not_accessible,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
            raw_params: json_raw_params(json),
        })
    }

    /// Parses a `SchLib` elliptical arc from JSON. Requires centre and both
    /// radii; optionals default as
    /// [`crate::altium::schlib::EllipticalArc::new`] does when absent.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_elliptical_arc(
        json: &Value,
    ) -> Option<crate::altium::schlib::EllipticalArc> {
        use crate::altium::schlib::EllipticalArc;

        let x = json_f64(json, "x")?;
        let y = json_f64(json, "y")?;
        let radius = json_f64(json, "radius")?;
        let secondary_radius = json_f64(json, "secondary_radius")?;
        let start_angle = json
            .get("start_angle")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let end_angle = json
            .get("end_angle")
            .and_then(Value::as_f64)
            .unwrap_or(360.0);
        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let fill_color = json.get("fill_color").and_then(Value::as_u64).unwrap_or(0) as u32;
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(EllipticalArc {
            x,
            y,
            radius,
            secondary_radius,
            start_angle,
            end_angle,
            line_width,
            color,
            fill_color,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
            raw_params: json_raw_params(json),
        })
    }

    /// Parses a schematic ellipse from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_ellipse(json: &Value) -> Option<crate::altium::schlib::Ellipse> {
        use crate::altium::schlib::Ellipse;

        let x = json_f64(json, "x")?;
        let y = json_f64(json, "y")?;
        let radius_x = json_f64(json, "radius_x")?;
        let radius_y = json_f64(json, "radius_y")?;

        let line_width = json.get("line_width").and_then(Value::as_u64).unwrap_or(1) as u8;
        let line_color = json
            .get("line_color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let fill_color = json
            .get("fill_color")
            .and_then(Value::as_u64)
            .unwrap_or(0xB0_FF_FF) as u32;
        let filled = json.get("filled").and_then(Value::as_bool).unwrap_or(true);
        // `transparent` read from JSON (matches the name
        // `read_schlib` exposes). The ellipse struct carries no `line_style`.
        let transparent = json
            .get("transparent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        // `is_not_accessible` read from JSON (matches
        // the name `read_schlib` exposes). Altium tags every ellipse, so
        // default true.
        let is_not_accessible = json
            .get("is_not_accessible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(Ellipse {
            x,
            y,
            radius_x,
            radius_y,
            line_width,
            line_color,
            fill_color,
            filled,
            transparent,
            is_not_accessible,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
            raw_params: json_raw_params(json),
        })
    }

    /// Parses a schematic label from JSON.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn parse_schlib_label(json: &Value) -> Result<crate::altium::schlib::Label, String> {
        use crate::altium::schlib::{Label, TextJustification};

        let number = |key: &str| {
            json_f64(json, key).ok_or_else(|| format!("Label is missing a numeric '{key}'"))
        };
        let x = number("x")?;
        let y = number("y")?;
        let text = json
            .get("text")
            .and_then(Value::as_str)
            .ok_or("Label is missing its 'text' string")?
            .to_string();

        let font_id = json.get("font_id").and_then(Value::as_u64).unwrap_or(1) as u8;
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .unwrap_or(0x00_00_80) as u32;
        let rotation = json.get("rotation").and_then(Value::as_f64).unwrap_or(0.0);
        let is_mirrored = json
            .get("is_mirrored")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let is_hidden = json
            .get("is_hidden")
            .or_else(|| json.get("hidden"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        let justification: TextJustification = enum_field(
            json,
            "justification",
            "Label justification",
            accepted::TEXT_JUSTIFICATIONS,
            accepted::TEXT_JUSTIFICATION_SYNONYMS,
        )?
        .unwrap_or(TextJustification::BottomLeft);

        Ok(Label {
            x,
            y,
            text,
            font_id,
            color,
            justification,
            rotation,
            is_mirrored,
            is_hidden,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            unique_id: json_unique_id(json),
            raw_params: json_raw_params(json),
        })
    }

    /// Parses a schematic IEEE symbol (RECORD=3) from JSON. `symbol` is
    /// Altium's `TIeeeSymbol` id (1 Dot, 3 Clock, …); see
    /// `docs/SCHLIB_FORMAT.md` for the table.
    pub(crate) fn parse_schlib_ieee_symbol(
        json: &Value,
    ) -> Option<crate::altium::schlib::IeeeSymbol> {
        use crate::altium::schlib::IeeeSymbol;

        let x = json_f64(json, "x")?;
        let y = json_f64(json, "y")?;
        let symbol = json
            .get("symbol")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())?;
        let scale_factor = json_f64(json, "scale_factor").unwrap_or(10.0);
        let rotation = json_f64(json, "rotation").unwrap_or(0.0);
        let is_mirrored = json
            .get("is_mirrored")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let line_width = json
            .get("line_width")
            .and_then(Value::as_u64)
            .and_then(|v| u8::try_from(v).ok())
            .unwrap_or(1);
        let color = json
            .get("color")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0);
        let owner_part_id = json_i32(json, "owner_part_id").unwrap_or(1);

        Some(IeeeSymbol {
            x,
            y,
            symbol,
            scale_factor,
            rotation,
            is_mirrored,
            line_width,
            color,
            owner_part_id,
            display_flags: parse_schlib_display_flags(json),
            raw_params: json_raw_params(json),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{json_f64, json_i32};
    use crate::mcp::server::McpServer;
    use serde_json::json;

    #[test]
    fn key_order_is_taken_from_the_region_that_read_it() {
        // A read-modify-write hands the original key order straight back so the
        // block keeps its byte layout. Absent or malformed, the writer's own
        // canonical order applies instead — an empty list, not a guess.
        assert_eq!(
            McpServer::parse_key_order(&json!({
                "param_key_order": ["VERSION", "UNITS", "DATE"]
            })),
            vec!["VERSION", "UNITS", "DATE"]
        );

        // Non-string entries are dropped rather than stringified.
        assert_eq!(
            McpServer::parse_key_order(&json!({ "param_key_order": ["A", 7, null, "B"] })),
            vec!["A", "B"]
        );

        for absent in [json!({}), json!({ "param_key_order": null })] {
            assert!(McpServer::parse_key_order(&absent).is_empty(), "{absent}");
        }
    }

    #[test]
    fn json_i32_drops_fractional_coordinate() {
        // Demonstrates the original defect: the integer reader rejects an
        // off-grid value, so a fractional coordinate was silently dropped while
        // an integer one passed through.
        assert_eq!(json_i32(&json!({ "x": 3.75 }), "x"), None);
        assert_eq!(json_i32(&json!({ "x": 3 }), "x"), Some(3));
    }

    #[test]
    fn json_f64_accepts_numbers_and_rejects_non_numeric() {
        // The fix: accept fractional and integer JSON numbers; reject non-numeric.
        assert_eq!(json_f64(&json!({ "x": 3.75 }), "x"), Some(3.75));
        assert_eq!(json_f64(&json!({ "x": -28 }), "x"), Some(-28.0));
        assert_eq!(json_f64(&json!({ "x": "nope" }), "x"), None);
        assert_eq!(json_f64(&json!({}), "x"), None);
    }

    #[test]
    fn parse_schlib_line_preserves_fractional_coords() {
        // End-to-end: a fractional line now parses (instead of being dropped)
        // and keeps its exact coordinates, including a negative fractional X.
        let line = McpServer::parse_schlib_line(&json!({
            "x1": -28.995, "y1": 7.5, "x2": 10.0, "y2": 0.0
        }))
        .expect("fractional line should parse");
        assert!((line.x1 - (-28.995)).abs() < 1e-9, "x1 kept: {}", line.x1);
        assert!((line.y1 - 7.5).abs() < 1e-9, "y1 kept: {}", line.y1);
        assert!((line.x2 - 10.0).abs() < 1e-9, "x2 kept: {}", line.x2);
    }

    // --- PR-4: flags / solder_mask_expansion / keepout_restrictions on the
    // write (JSON -> primitive) path. The `flags` JSON shape is the raw u16
    // bitmask that `read_pcblib` serialises (PcbFlags is #[serde(transparent)]),
    // so the values these tests feed in are the same ones a read would emit.

    #[test]
    fn json_flags_reads_read_dto_string_form() {
        use crate::altium::pcblib::PcbFlags;
        // Canonical round-trip shape: the bitflags serde string read_pcblib emits.
        let flags = super::json_flags(&json!({ "flags": "LOCKED | KEEPOUT" }));
        assert!(flags.contains(PcbFlags::LOCKED));
        assert!(flags.contains(PcbFlags::KEEPOUT));
        let single = super::json_flags(&json!({ "flags": "LOCKED" }));
        assert!(single.contains(PcbFlags::LOCKED));
        assert!(!single.contains(PcbFlags::KEEPOUT));
        // Convenience shape: a raw bitmask integer (LOCKED 1 | KEEPOUT 4 = 5).
        let int_flags = super::json_flags(&json!({ "flags": 5 }));
        assert!(int_flags.contains(PcbFlags::LOCKED));
        assert!(int_flags.contains(PcbFlags::KEEPOUT));
        // Absent key -> empty (default), matching the read-side skip_serializing_if.
        assert!(super::json_flags(&json!({})).is_empty());
    }

    #[test]
    fn pcbflags_write_then_read_dto_round_trip() {
        use crate::altium::pcblib::PcbFlags;
        // The string the read DTO serialises must parse back to the same flags on
        // the write path — guards the read/write shape reconciliation.
        let original = PcbFlags::LOCKED | PcbFlags::KEEPOUT;
        let dto = serde_json::to_value(original).expect("serialise flags");
        let parsed = super::json_flags(&json!({ "flags": dto }));
        assert_eq!(parsed, original);
    }

    #[test]
    fn parse_pad_reads_flags_and_solder_mask() {
        use crate::altium::pcblib::{MaskExpansionMode, PcbFlags};
        let pad = McpServer::parse_pad(&json!({
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
            "flags": "LOCKED",
            "solder_mask_expansion": 0.05,
            "solder_mask_expansion_mode": "manual",
        }))
        .expect("pad should parse");
        assert!(pad.flags.contains(PcbFlags::LOCKED));
        assert_eq!(pad.solder_mask_expansion, Some(0.05));
        assert_eq!(pad.solder_mask_expansion_mode, MaskExpansionMode::Manual);
    }

    #[test]
    fn parse_pad_reads_plating_and_identity_guids() {
        // is_plated @60 and the two identity GUIDs @126/@142 flow from JSON so
        // a read-modify-write preserves them.
        let pad = McpServer::parse_pad(&json!({
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
            "is_plated": false,
            "identity_guid": "{A5172B29-10E4-C726-929A-64E441352E67}",
            "identity_guid_b": "{00000000-0000-0000-0000-000000000000}",
        }))
        .expect("pad should parse");
        assert!(!pad.is_plated);
        assert_eq!(
            pad.identity_guid.as_deref(),
            Some("{A5172B29-10E4-C726-929A-64E441352E67}")
        );
        assert_eq!(
            pad.identity_guid_b.as_deref(),
            Some("{00000000-0000-0000-0000-000000000000}")
        );

        // Absent keys keep the from-scratch defaults: plated (Altium's default
        // for every pad) and fresh writer-generated GUIDs (None).
        let bare = McpServer::parse_pad(&json!({
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
        }))
        .expect("bare pad should parse");
        assert!(bare.is_plated);
        assert_eq!(bare.identity_guid, None);
        assert_eq!(bare.identity_guid_b, None);
    }

    #[test]
    fn parse_pad_reads_polygon_connect() {
        use crate::altium::pcblib::{PolygonConnect, PowerPlaneConnectStyle};

        let pad = |connect: serde_json::Value| {
            McpServer::parse_pad(&json!({
                "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
                "polygon_connect": connect,
            }))
        };
        assert_eq!(
            pad(serde_json::Value::Null).expect("null").polygon_connect,
            None,
            "null follows the rules"
        );
        assert_eq!(
            pad(json!({})).expect("empty").polygon_connect,
            Some(PolygonConnect::default()),
            "an empty object is the override Altium writes on ticking the box"
        );
        let full = pad(json!({
            "style": "no_connect", "air_gap": 0.2, "conductor_width": 0.3,
            "conductors": 2, "auto_conductors": true, "rotation": 45, "min_distance": 0.5,
            "min_distance_enabled": true,
        }))
        .expect("full")
        .polygon_connect
        .expect("override");
        assert_eq!(
            full,
            PolygonConnect {
                style: PowerPlaneConnectStyle::NoConnect,
                air_gap: 0.2,
                conductor_width: 0.3,
                conductors: 2,
                auto_conductors: true,
                rotation: 45,
                min_distance: 0.5,
                min_distance_enabled: true,
            }
        );
        for (bad, needle) in [
            (json!(true), "must be an object"),
            (json!({ "conductors": 3 }), "conductors must be 2 or 4"),
            (json!({ "rotation": 30 }), "rotation must be 45 or 90"),
            (json!({ "air_gap": -0.1 }), "air_gap must be a length"),
            (
                json!({ "min_distance": "x" }),
                "min_distance must be a length",
            ),
            (
                json!({ "auto_conductors": 1 }),
                "auto_conductors must be true or false",
            ),
            (
                json!({ "min_distance_enabled": "yes" }),
                "min_distance_enabled must be true or false",
            ),
            (json!({ "style": "solid" }), "style"),
        ] {
            let err = pad(bad.clone()).expect_err(&format!("{bad} must be refused"));
            assert!(err.contains(needle), "{bad}: {err}");
            assert!(err.contains("Pad '1' polygon_connect"), "{bad}: {err}");
        }
    }

    #[test]
    fn parse_via_reads_polygon_connect() {
        use crate::altium::pcblib::{PolygonConnect, PowerPlaneConnectStyle};

        let via = McpServer::parse_via(&json!({
            "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3,
            "polygon_connect": { "style": "direct" },
        }))
        .expect("via");
        assert_eq!(
            via.polygon_connect,
            Some(PolygonConnect {
                style: PowerPlaneConnectStyle::Direct,
                ..PolygonConnect::default()
            })
        );
        let err = McpServer::parse_via(&json!({
            "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3,
            "polygon_connect": { "conductors": 3 },
        }))
        .expect_err("3 conductors");
        assert!(
            err.contains("Via polygon_connect.conductors must be 2 or 4"),
            "{err}"
        );
    }

    #[test]
    fn parse_pad_reads_thermal_relief_fields() {
        use crate::altium::pcblib::PowerPlaneConnectStyle;
        // Non-default thermal-relief / power-plane keys parse into the model.
        let pad = McpServer::parse_pad(&json!({
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
            "power_plane_connect_style": "direct",
            "relief_conductor_width": 0.3,
            "relief_entries": 2,
            "relief_air_gap": 0.2,
            "power_plane_relief_expansion": 0.6,
            "power_plane_clearance": 0.7,
        }))
        .expect("pad should parse");
        assert_eq!(
            pad.power_plane_connect_style,
            PowerPlaneConnectStyle::Direct
        );
        assert!((pad.relief_conductor_width - 0.3).abs() < 1e-9);
        assert_eq!(pad.relief_entries, 2);
        assert!((pad.relief_air_gap - 0.2).abs() < 1e-9);
        assert!((pad.power_plane_relief_expansion - 0.6).abs() < 1e-9);
        assert!((pad.power_plane_clearance - 0.7).abs() < 1e-9);
    }

    #[test]
    fn parse_pad_thermal_relief_defaults() {
        use crate::altium::pcblib::PowerPlaneConnectStyle;
        // Absent keys keep the from-scratch defaults (= Altium's pad template).
        let pad = McpServer::parse_pad(&json!({
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
        }))
        .expect("pad should parse");
        assert_eq!(
            pad.power_plane_connect_style,
            PowerPlaneConnectStyle::Relief
        );
        assert!((pad.relief_conductor_width - 0.254).abs() < 1e-9);
        assert_eq!(pad.relief_entries, 4);
        assert!((pad.relief_air_gap - 0.254).abs() < 1e-9);
        assert!((pad.power_plane_relief_expansion - 0.508).abs() < 1e-9);
        assert!((pad.power_plane_clearance - 0.508).abs() < 1e-9);
    }

    #[test]
    fn parse_via_reads_power_plane_and_flags() {
        use crate::altium::pcblib::{PcbFlags, PowerPlaneConnectStyle};
        // PR-7: power-plane connection, paste-mask, net index and flags parse in.
        let via = McpServer::parse_via(&json!({
            "x": 0.0, "y": 0.0, "diameter": 0.8, "hole_size": 0.4,
            "power_plane_connect_style": "direct",
            "power_plane_relief_expansion": 0.6,
            "power_plane_clearance": 0.7,
            "paste_mask_expansion": 0.05,
            "net_index": 42,
            "flags": "TENTING_TOP | LOCKED",
        }))
        .expect("via should parse");
        assert_eq!(
            via.power_plane_connect_style,
            PowerPlaneConnectStyle::Direct
        );
        assert!((via.power_plane_relief_expansion - 0.6).abs() < 1e-9);
        assert!((via.power_plane_clearance - 0.7).abs() < 1e-9);
        assert!((via.paste_mask_expansion - 0.05).abs() < 1e-9);
        assert_eq!(via.net_index, 42);
        assert!(via.flags.contains(PcbFlags::TENTING_TOP));
        assert!(via.flags.contains(PcbFlags::LOCKED));
    }

    #[test]
    fn parse_via_defaults_match_template() {
        use crate::altium::pcblib::{PcbFlags, PowerPlaneConnectStyle};
        // Absent keys keep the from-scratch defaults (= Altium's via template).
        let via = McpServer::parse_via(&json!({
            "x": 0.0, "y": 0.0, "diameter": 0.8, "hole_size": 0.4,
        }))
        .expect("via should parse");
        assert_eq!(
            via.power_plane_connect_style,
            PowerPlaneConnectStyle::Relief
        );
        assert!((via.power_plane_relief_expansion - 0.508).abs() < 1e-9);
        assert!((via.power_plane_clearance - 0.508).abs() < 1e-9);
        assert!((via.paste_mask_expansion - 0.0).abs() < 1e-9);
        assert_eq!(via.net_index, 0xFFFF);
        assert_eq!(via.flags, PcbFlags::empty());
    }

    #[test]
    fn parse_track_reads_flags_solder_mask_keepout() {
        use crate::altium::pcblib::PcbFlags;
        let track = McpServer::parse_track(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.15,
            "layer": "Top Overlay",
            "flags": "KEEPOUT",
            "solder_mask_expansion": 0.1,
            "keepout_restrictions": 3,
        }))
        .expect("track should parse");
        assert!(track.flags.contains(PcbFlags::KEEPOUT));
        assert_eq!(track.solder_mask_expansion, Some(0.1));
        assert_eq!(track.keepout_restrictions, Some(3));
        // Absent keys leave the Track::new defaults untouched.
        let bare = McpServer::parse_track(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.15, "layer": "Top Overlay"
        }))
        .expect("bare track should parse");
        assert!(bare.flags.is_empty());
        assert_eq!(bare.solder_mask_expansion, None);
        assert_eq!(bare.keepout_restrictions, None);
    }

    #[test]
    fn parse_arc_reads_flags_solder_mask_keepout() {
        use crate::altium::pcblib::PcbFlags;
        let arc = McpServer::parse_arc(&json!({
            "x": 0.0, "y": 0.0, "radius": 1.0,
            "start_angle": 0.0, "end_angle": 90.0, "width": 0.15,
            "layer": "Top Overlay",
            "flags": "LOCKED",
            "solder_mask_expansion": 0.2,
            "keepout_restrictions": 5,
        }))
        .expect("arc should parse");
        assert!(arc.flags.contains(PcbFlags::LOCKED));
        assert_eq!(arc.solder_mask_expansion, Some(0.2));
        assert_eq!(arc.keepout_restrictions, Some(5));
    }

    #[test]
    fn parse_region_reads_flags() {
        use crate::altium::pcblib::PcbFlags;
        let region = McpServer::parse_region(&json!({
            "layer": "Top Courtyard",
            "vertices": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 0.0}, {"x": 0.0, "y": 1.0}],
            "flags": "KEEPOUT",
        }))
        .expect("region should parse");
        assert!(region.flags.contains(PcbFlags::KEEPOUT));
    }

    #[test]
    fn parse_region_reads_additional_parameters() {
        // PR-R5: the read DTO's `additional_parameters` (an array of [key, value]
        // pairs) must land on the struct so a read-modify-write preserves them.
        let region = McpServer::parse_region(&json!({
            "layer": "Top Courtyard",
            "vertices": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 0.0}, {"x": 0.0, "y": 1.0}],
            "additional_parameters": [["LAYER", "TOP"], ["LAYERSTACKID", "7"]],
        }))
        .expect("region should parse");
        assert_eq!(
            region.additional_parameters,
            vec![
                ("LAYER".to_string(), "TOP".to_string()),
                ("LAYERSTACKID".to_string(), "7".to_string()),
            ],
        );
    }

    #[test]
    fn parse_region_additional_parameters_default_empty() {
        // Absent -> empty, so a from-scratch region re-emits nothing (byte-identical).
        let region = McpServer::parse_region(&json!({
            "layer": "Top Courtyard",
            "vertices": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 0.0}, {"x": 0.0, "y": 1.0}],
        }))
        .expect("region should parse");
        assert!(region.additional_parameters.is_empty());
    }

    #[test]
    fn parse_fill_reads_flags() {
        use crate::altium::pcblib::PcbFlags;
        let fill = McpServer::parse_fill(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 1.0, "layer": "Top Layer",
            "flags": "LOCKED",
            "solder_mask_expansion": 0.05,
            "keepout_restrictions": 2,
        }))
        .expect("fill should parse");
        assert!(fill.flags.contains(PcbFlags::LOCKED));
        assert_eq!(fill.solder_mask_expansion, Some(0.05));
        assert_eq!(fill.keepout_restrictions, Some(2));
    }

    #[test]
    fn parse_text_reads_flags() {
        use crate::altium::pcblib::PcbFlags;
        let text = McpServer::parse_text(&json!({
            "x": 0.0, "y": 0.0, "text": "REF", "height": 0.5, "layer": "Top Overlay",
            "flags": "LOCKED",
        }))
        .expect("text should parse");
        assert!(text.flags.contains(PcbFlags::LOCKED));
    }

    #[test]
    fn parse_text_reads_authoring_fields() {
        // kind/stroke_font/italic/bold/mirror/font_name/justification must each
        // flow from JSON onto the struct.
        use crate::altium::pcblib::{StrokeFont, TextJustification, TextKind};
        let text = McpServer::parse_text(&json!({
            "x": 0.0, "y": 0.0, "text": "REF", "height": 0.5, "layer": "Top Overlay",
            "kind": "true_type",
            "stroke_font": "serif",
            "italic": true,
            "bold": true,
            "mirror": true,
            "is_comment": true,
            "is_designator": true,
            "font_name": "Times New Roman",
            "justification": "top_right",
        }))
        .expect("text should parse");
        assert_eq!(text.kind, TextKind::TrueType);
        assert_eq!(text.stroke_font, Some(StrokeFont::Serif));
        assert!(text.italic);
        assert!(text.bold);
        assert!(text.mirror);
        assert!(text.is_comment);
        assert!(text.is_designator);
        assert_eq!(text.font_name, "Times New Roman");
        assert_eq!(text.justification, TextJustification::TopRight);
    }

    #[test]
    fn parse_text_defaults_are_template_identical() {
        // A minimal text must keep the from-scratch defaults (stroke, no font
        // override, Arial, middle-center) so it stays byte-identical on write.
        use crate::altium::pcblib::{TextJustification, TextKind};
        let text = McpServer::parse_text(&json!({
            "x": 0.0, "y": 0.0, "text": "REF", "height": 0.5, "layer": "Top Overlay",
        }))
        .expect("text should parse");
        assert_eq!(text.kind, TextKind::Stroke);
        assert_eq!(text.stroke_font, None);
        assert!(!text.italic);
        assert!(!text.bold);
        assert!(!text.mirror);
        assert!(!text.is_comment, "absent is_comment stays template false");
        assert!(
            !text.is_designator,
            "absent is_designator stays template false"
        );
        assert_eq!(text.font_name, "Arial");
        assert_eq!(text.justification, TextJustification::BottomLeft);
    }

    // --- SchLib write-path authoring fields. Each must reach the struct from
    // JSON on write, not just round-trip on read. Each test sets a non-default

    // asserts it lands on the struct (the field names match the read DTO).

    #[test]
    fn parse_schlib_pin_reads_authoring_fields() {
        let pin = McpServer::parse_schlib_pin(&json!({
            "designator": "1", "name": "P1", "x": 0, "y": 0, "length": 10,
            "orientation": "left",
            "description": "clock input",
            "colour": 0x00_FF_00,
            "graphically_locked": true,
            "swap_id_group": "grpA",
            "part_and_sequence": "|1&2|",
            "default_value": "0",
            "owner_part_display_mode": 2,
            "symbol_line_width": 3,
            "frac": { "x": 50000, "y": -25000, "length": 0 },
        }))
        .expect("pin should parse");
        assert_eq!(pin.description, "clock input");
        assert_eq!(pin.colour, 0x00_FF_00);
        assert!(pin.graphically_locked);
        assert_eq!(pin.swap_id_group, "grpA");
        assert_eq!(pin.part_and_sequence, "|1&2|");
        assert_eq!(pin.default_value, "0");
        assert_eq!(pin.owner_part_display_mode, 2);
        assert_eq!(pin.symbol_line_width, 3);
        assert_eq!(
            pin.frac,
            Some(crate::altium::schlib::PinFrac {
                x: 50000,
                y: -25000,
                length: 0
            })
        );
    }

    #[test]
    fn parse_schlib_pin_defaults_match_struct() {
        // Absent authoring keys keep the from-scratch defaults (notably the
        // `|&|` part_and_sequence Altium uses for a fresh pin).
        let pin = McpServer::parse_schlib_pin(&json!({
            "designator": "1", "name": "P1", "x": 0, "y": 0, "length": 10,
            "orientation": "left",
        }))
        .expect("pin should parse");
        assert_eq!(pin.description, "");
        assert_eq!(pin.colour, 0);
        assert!(!pin.graphically_locked);
        assert_eq!(pin.swap_id_group, "");
        assert_eq!(pin.part_and_sequence, "|&|");
        assert_eq!(pin.default_value, "");
        // PR-R3 aux fields default so no aux stream is written for a plain pin.
        assert_eq!(pin.owner_part_display_mode, 0);
        assert_eq!(pin.symbol_line_width, 0);
        assert_eq!(pin.frac, None);
    }

    #[test]
    fn parse_schlib_pin_reads_open_collector_electrical_type() {
        use crate::altium::schlib::PinElectricalType;
        let oc = McpServer::parse_schlib_pin(&json!({
            "designator": "1", "name": "P1", "x": 0, "y": 0, "length": 10,
            "orientation": "left", "electrical_type": "open_collector",
        }))
        .expect("pin should parse");
        assert_eq!(oc.electrical_type, PinElectricalType::OpenCollector);
        // `tristate` is the advertised alias for HiZ.
        let tri = McpServer::parse_schlib_pin(&json!({
            "designator": "2", "name": "P2", "x": 0, "y": 0, "length": 10,
            "orientation": "left", "electrical_type": "tristate",
        }))
        .expect("pin should parse");
        assert_eq!(tri.electrical_type, PinElectricalType::HiZ);
    }

    #[test]
    fn parse_schlib_rectangle_reads_line_style_and_transparent() {
        let rect = McpServer::parse_schlib_rectangle(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 10.0, "y2": 10.0,
            "line_style": 2, "transparent": true,
        }))
        .expect("rectangle should parse");
        assert_eq!(rect.line_style, 2);
        assert!(rect.transparent);
    }

    #[test]
    fn parse_schlib_round_rect_reads_line_style_and_transparent() {
        let rr = McpServer::parse_schlib_round_rect(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 10.0, "y2": 10.0,
            "corner_x_radius": 2.0, "corner_y_radius": 2.0,
            "line_style": 1, "transparent": true,
        }))
        .expect("round_rect should parse");
        assert_eq!(rr.line_style, 1);
        assert!(rr.transparent);
    }

    #[test]
    fn parse_schlib_line_reads_line_style() {
        let line = McpServer::parse_schlib_line(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 10.0, "y2": 0.0, "line_style": 2,
        }))
        .expect("line should parse");
        assert_eq!(line.line_style, 2);
    }

    #[test]
    fn parse_schlib_polyline_reads_style_and_arrowheads() {
        let pl = McpServer::parse_schlib_polyline(&json!({
            "points": [{"x": 0.0, "y": 0.0}, {"x": 10.0, "y": 0.0}],
            "line_style": 1,
            "start_line_shape": 2,
            "end_line_shape": 3,
            "line_shape_size": 4,
            "transparent": true,
        }))
        .expect("polyline should parse");
        assert_eq!(pl.line_style, 1);
        assert_eq!(pl.start_line_shape, 2);
        assert_eq!(pl.end_line_shape, 3);
        assert_eq!(pl.line_shape_size, 4);
        assert!(pl.transparent);
    }

    #[test]
    fn parse_schlib_arc_reads_fill_color() {
        let arc = McpServer::parse_schlib_arc(&json!({
            "x": 0.0, "y": 0.0, "radius": 5.0, "fill_color": 0x11_22_33,
        }))
        .expect("arc should parse");
        assert_eq!(arc.fill_color, 0x11_22_33);
    }

    #[test]
    fn parse_schlib_ellipse_reads_transparent() {
        let el = McpServer::parse_schlib_ellipse(&json!({
            "x": 0.0, "y": 0.0, "radius_x": 5.0, "radius_y": 3.0, "transparent": true,
        }))
        .expect("ellipse should parse");
        assert!(el.transparent);
    }

    #[test]
    fn parse_schlib_ellipse_and_polyline_read_is_not_accessible() {
        // Absent defaults true (Altium tags every ellipse/polyline); an explicit
        // false must round-trip through the JSON write path.
        let el = McpServer::parse_schlib_ellipse(&json!({
            "x": 0.0, "y": 0.0, "radius_x": 5.0, "radius_y": 3.0,
        }))
        .expect("ellipse should parse");
        assert!(el.is_not_accessible, "ellipse defaults true");
        let el = McpServer::parse_schlib_ellipse(&json!({
            "x": 0.0, "y": 0.0, "radius_x": 5.0, "radius_y": 3.0,
            "is_not_accessible": false,
        }))
        .expect("ellipse should parse");
        assert!(!el.is_not_accessible, "explicit false is honoured");

        let points = json!([{ "x": 0.0, "y": 0.0 }, { "x": 5.0, "y": 5.0 }]);
        let pl = McpServer::parse_schlib_polyline(&json!({ "points": points }))
            .expect("polyline should parse");
        assert!(pl.is_not_accessible, "polyline defaults true");
        let pl = McpServer::parse_schlib_polyline(
            &json!({ "points": points, "is_not_accessible": false }),
        )
        .expect("polyline should parse");
        assert!(!pl.is_not_accessible, "explicit false is honoured");
    }

    #[test]
    fn parse_schlib_parameter_reads_justification() {
        // Altium anchor id 0-8 (golden JUSTIFY: 8 = top-right); absent = 0.
        let p = McpServer::parse_schlib_parameter(&json!({
            "name": "Value", "value": "10k", "justification": 8,
        }))
        .expect("parameter should parse");
        assert_eq!(p.justification, 8);
        let p = McpServer::parse_schlib_parameter(&json!({ "name": "Value" }))
            .expect("parameter should parse");
        assert_eq!(p.justification, 0, "absent justification defaults to 0");
    }

    // --- PR-R1: round-trip preservation of a primitive's `unique_id` (identity
    // GUID). An absent `unique_id` MUST stay `None` (the writer then

    // `unique_id` MUST stay `None` (the writer then auto-generates, keeping
    // from-scratch output byte-identical).

    #[test]
    fn json_unique_id_reads_and_defaults() {
        assert_eq!(
            super::json_unique_id(&json!({ "unique_id": "QHHMRSCB" })).as_deref(),
            Some("QHHMRSCB")
        );
        // Absent -> None, so the writer auto-generates exactly as before.
        assert_eq!(super::json_unique_id(&json!({})), None);
    }

    #[test]
    fn parse_via_preserves_provided_unique_id() {
        let via = McpServer::parse_via(&json!({
            "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3,
            "unique_id": "VIAUID01",
        }))
        .expect("via should parse");
        assert_eq!(via.unique_id.as_deref(), Some("VIAUID01"));
    }

    #[test]
    fn parse_via_without_unique_id_defaults_none() {
        // From-scratch: no unique_id -> None (writer auto-generates; byte-identical).
        let via = McpServer::parse_via(&json!({
            "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3,
        }))
        .expect("via should parse");
        assert_eq!(via.unique_id, None);
    }

    #[test]
    fn parse_component_body_reads_raw_layer_pair() {
        // #391: the unmapped-byte pair read_pcblib echoes must survive the
        // tool-layer JSON boundary like every other replay field.
        let body = McpServer::parse_component_body_json(&json!({
            "model_id": "{G-1}",
            "layer": "Multi-Layer",
            "raw_layer_id": 150,
            "v7_layer": "MECHANICAL22",
        }))
        .expect("body should parse");
        assert_eq!(body.raw_layer_id, Some(150));
        assert_eq!(body.v7_layer.as_deref(), Some("MECHANICAL22"));

        // Absent -> None, out-of-range -> None (lenient Option-style).
        let plain = McpServer::parse_component_body_json(&json!({
            "model_id": "{G-1}",
            "raw_layer_id": 300,
        }))
        .expect("body should parse");
        assert_eq!(plain.raw_layer_id, None);
        assert_eq!(plain.v7_layer, None);
    }

    #[test]
    fn json_base64_decodes_and_ignores_invalid() {
        // "AAEC" is [0, 1, 2]; invalid base64 and absent keys both read None,
        // so the writer falls back to its template instead of erroring.
        assert_eq!(
            super::json_base64(&json!({ "raw": "AAEC" }), "raw"),
            Some(vec![0u8, 1, 2])
        );
        assert_eq!(super::json_base64(&json!({ "raw": "!!!" }), "raw"), None);
        assert_eq!(super::json_base64(&json!({}), "raw"), None);
    }

    #[test]
    fn parse_pad_preserves_raw_tail() {
        let pad = McpServer::parse_pad(&json!({
            "designator": "1", "x": 0.0, "y": 0.0, "width": 0.6, "height": 0.5,
            "raw_tail": "AAECAwQ=",
        }))
        .expect("pad should parse");
        assert_eq!(pad.raw_tail.as_deref(), Some(&[0u8, 1, 2, 3, 4][..]));
    }

    #[test]
    fn parse_via_reads_diameter_stack_and_back_mask() {
        use crate::altium::pcblib::ViaStackMode;
        let mut diameters = vec![0.6; 32];
        diameters[1] = 0.7;
        let via = McpServer::parse_via(&json!({
            "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3,
            "diameter_stack_mode": "full_stack",
            "per_layer_diameters": diameters,
            "solder_mask_expansion_back": 0.05,
        }))
        .expect("via should parse");
        assert_eq!(via.diameter_stack_mode, ViaStackMode::FullStack);
        let read = via.per_layer_diameters.expect("diameters read");
        assert_eq!(read.len(), 32);
        assert_eq!((read[0], read[1], read[2]), (0.6, 0.7, 0.6));
        assert_eq!(via.solder_mask_expansion_back, Some(0.05));

        // Absent -> struct defaults, so a from-scratch via is unchanged.
        let plain = McpServer::parse_via(&json!({
            "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3,
        }))
        .expect("via should parse");
        assert_eq!(plain.diameter_stack_mode, ViaStackMode::Simple);
        assert_eq!(plain.per_layer_diameters, None);
        assert_eq!(plain.solder_mask_expansion_back, None);
    }

    /// A via's diameter stack is held to the record's 32 slots: fewer or
    /// more entries, an entry that is not a number, or a stack on a simple
    /// via is refused rather than filled from `diameter` or ignored.
    #[test]
    fn parse_via_refuses_a_diameter_stack_the_record_cannot_hold() {
        let with = |mode: &str, diameters: serde_json::Value| {
            McpServer::parse_via(&json!({
                "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3,
                "diameter_stack_mode": mode, "per_layer_diameters": diameters,
            }))
            .unwrap_err()
        };
        let err = with("full_stack", json!([0.6, 0.7, 0.8]));
        assert!(
            err.contains(
                "Via per_layer_diameters has 3 entries; a stacked via takes 32 (index 0 = Top, \
                 1 = Bottom, 2-31 = Mid layers)"
            ),
            "{err}"
        );
        let err = with("simple", json!(vec![0.6; 32]));
        assert!(
            err.contains("Via per_layer_diameters is given but diameter_stack_mode is simple"),
            "{err}"
        );
        let mut entries = vec![json!(0.6); 32];
        entries[5] = json!("thick");
        let err = with("top_middle_bottom", json!(entries));
        assert!(
            err.contains("Via per_layer_diameters[5] must be a number, got \"thick\""),
            "{err}"
        );
        let err = with("full_stack", json!("wide"));
        assert!(
            err.contains("Via per_layer_diameters must be an array, got \"wide\""),
            "{err}"
        );
    }

    #[test]
    fn parse_via_preserves_guid_and_raw_block() {
        let via = McpServer::parse_via(&json!({
            "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3,
            "guid": "{01234567-89AB-CDEF-0123-456789ABCDEF}",
            "raw_block": "BQYH",
        }))
        .expect("via should parse");
        assert_eq!(
            via.guid.as_deref(),
            Some("{01234567-89AB-CDEF-0123-456789ABCDEF}")
        );
        assert_eq!(via.raw_block.as_deref(), Some(&[5u8, 6, 7][..]));
    }

    #[test]
    fn parse_text_preserves_raw_geometry() {
        let text = McpServer::parse_text(&json!({
            "text": "REF", "x": 0.0, "y": 0.0, "height": 1.0, "layer": "Top Overlay",
            "raw_geometry": "CAkK",
        }))
        .expect("text should parse");
        assert_eq!(text.raw_geometry.as_deref(), Some(&[8u8, 9, 10][..]));
    }

    #[test]
    fn parse_track_and_fill_preserve_guid() {
        let guid = "{FEDCBA98-7654-3210-FEDC-BA9876543210}";
        let track = McpServer::parse_track(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.2,
            "layer": "Top Overlay", "guid": guid,
        }))
        .expect("track should parse");
        assert_eq!(track.guid.as_deref(), Some(guid));

        let fill = McpServer::parse_fill(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 1.0,
            "layer": "Top Layer", "guid": guid,
        }))
        .expect("fill should parse");
        assert_eq!(fill.guid.as_deref(), Some(guid));
    }

    #[test]
    fn every_layered_primitive_carries_its_read_layer_byte() {
        // An unmapped header byte read onto the Multi-Layer catch-all
        // survives the JSON boundary on each of the five kinds that carry one.
        let with = |fields: serde_json::Value| {
            let mut json = fields;
            json["layer"] = json!("Multi-Layer");
            json["raw_layer_id"] = json!(100);
            json
        };
        let pad = McpServer::parse_pad(&with(json!({
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
        })))
        .expect("pad");
        let track = McpServer::parse_track(&with(json!({
            "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.2,
        })))
        .expect("track");
        let arc = McpServer::parse_arc(&with(json!({
            "x": 0.0, "y": 0.0, "radius": 1.0, "start_angle": 0.0, "end_angle": 90.0, "width": 0.2,
        })))
        .expect("arc");
        let text = McpServer::parse_text(&with(json!({
            "x": 0.0, "y": 0.0, "text": "T", "height": 1.0,
        })))
        .expect("text");
        let fill = McpServer::parse_fill(&with(json!({
            "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 1.0,
        })))
        .expect("fill");
        assert_eq!(
            [
                pad.raw_layer_id,
                track.raw_layer_id,
                arc.raw_layer_id,
                text.raw_layer_id,
                fill.raw_layer_id,
            ],
            [Some(100); 5]
        );
        assert_eq!(track.layer, crate::altium::pcblib::Layer::MultiLayer);
        // An out-of-range value is not a byte.
        let plain = McpServer::parse_fill(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 1.0, "layer": "Top Layer", "raw_layer_id": 300,
        }))
        .expect("fill");
        assert_eq!(plain.raw_layer_id, None);
    }

    #[test]
    fn parse_schlib_rectangle_preserves_provided_unique_id() {
        let rect = McpServer::parse_schlib_rectangle(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 10.0, "y2": 10.0,
            "unique_id": "RECTUID1",
        }))
        .expect("rectangle should parse");
        assert_eq!(rect.unique_id.as_deref(), Some("RECTUID1"));
        // From-scratch -> None (writer auto-generates; byte-identical).
        let plain = McpServer::parse_schlib_rectangle(&json!({
            "x1": 0.0, "y1": 0.0, "x2": 10.0, "y2": 10.0,
        }))
        .expect("rectangle should parse");
        assert_eq!(plain.unique_id, None);
    }

    #[test]
    fn pad_shape_vocabulary_is_shared_and_lenient() {
        use crate::altium::pcblib::PadShape;
        // Both tools resolve the same spellings. The pairs below span the two
        // documented vocabularies: `rounded_rectangle`/`circle` from
        // write_pcblib, `roundedrectangle`/`circular`/`rect` from update_pad.
        for (input, want) in [
            ("rectangle", PadShape::Rectangle),
            ("rect", PadShape::Rectangle),
            ("round", PadShape::Round),
            ("circle", PadShape::Round),
            ("circular", PadShape::Round),
            ("oval", PadShape::Oval),
            ("oblong", PadShape::Oval),
            ("octagonal", PadShape::Octagonal),
            ("octagon", PadShape::Octagonal),
            ("rounded_rectangle", PadShape::RoundedRectangle),
            ("roundedrectangle", PadShape::RoundedRectangle),
            ("rounded-rectangle", PadShape::RoundedRectangle),
            ("rounded", PadShape::RoundedRectangle),
            // case-insensitive: update_pad accepted these, write_pcblib did not
            ("Round", PadShape::Round),
            ("ROUNDED_RECTANGLE", PadShape::RoundedRectangle),
        ] {
            assert_eq!(
                McpServer::parse_pad_shape(input),
                Some(want),
                "shape {input:?} must resolve"
            );
        }
        assert_eq!(McpServer::parse_pad_shape("hexagon"), None);
    }

    #[test]
    fn parse_pad_shape_defaults_to_rounded_rectangle() {
        // Documents the default that BGA callers must override. A change here is a
        // silent geometry change for every caller that omits `shape`, so it should
        // be deliberate.
        use crate::altium::pcblib::PadShape;
        let pad = McpServer::parse_pad(&json!({
            "designator": "A1", "x": 0.0, "y": 0.0, "width": 0.3, "height": 0.3
        }))
        .expect("pad without shape should parse");
        assert_eq!(pad.shape, PadShape::RoundedRectangle);

        // A BGA land opting in explicitly stays circular.
        let bga = McpServer::parse_pad(&json!({
            "designator": "A1", "x": 0.0, "y": 0.0, "width": 0.3, "height": 0.3,
            "shape": "round"
        }))
        .expect("round pad should parse");
        assert_eq!(bga.shape, PadShape::Round);
    }

    // ==================== rejection paths and optional-field arms ============
    //
    // Every `parse_*` helper answers a malformed primitive with a message
    // naming the field, and silently defaults the optional ones. Both halves
    // are contract; the tests below pin each rejection and each default.

    mod rejections {
        use crate::mcp::server::McpServer;
        use serde_json::json;

        /// Asserts the parse failed and its message mentions `needle`.
        fn rejects<T: std::fmt::Debug>(result: Result<T, String>, needle: &str) {
            let err = result.expect_err("expected a rejection");
            assert!(
                err.contains(needle),
                "expected the message to mention {needle:?}, got: {err}"
            );
        }

        /// A pad payload that parses, so a test can drop or corrupt one field.
        fn pad_json() -> serde_json::Value {
            json!({ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 })
        }

        // ---- unknown-field guard ---------------------------------------------

        #[test]
        fn unknown_fields_are_named_along_with_what_was_allowed() {
            // A typo in an optional key would otherwise be silently ignored and
            // the caller would never learn the value did not take effect.
            let err = McpServer::check_unknown_fields(&json!({ "widht": 1.0 }), &["width"])
                .expect_err("an unknown field must be refused");
            assert!(err.contains("widht"), "{err}");
            assert!(err.contains("width"), "{err}");

            // A non-object carries no keys to check, and is left alone.
            assert!(McpServer::check_unknown_fields(&json!(42), &["width"]).is_ok());
            assert!(McpServer::check_unknown_fields(&json!({ "width": 1.0 }), &["width"]).is_ok());
        }

        // ---- parse_pad --------------------------------------------------------

        #[test]
        fn parse_pad_names_the_missing_required_field() {
            for field in ["designator", "x", "y", "width", "height"] {
                let mut json = pad_json();
                json.as_object_mut().unwrap().remove(field);
                rejects(McpServer::parse_pad(&json), field);
            }
        }

        #[test]
        fn parse_pad_rejects_an_empty_designator() {
            // Whitespace is not a designator: the pad would land in the library
            // unnameable and unmatched by any schematic pin.
            let mut json = pad_json();
            json["designator"] = json!("   ");
            rejects(McpServer::parse_pad(&json), "cannot be empty");
        }

        #[test]
        fn parse_pad_rejects_non_positive_dimensions() {
            for field in ["width", "height"] {
                let mut zero = pad_json();
                zero[field] = json!(0.0);
                rejects(McpServer::parse_pad(&zero), "greater than 0");

                let mut negative = pad_json();
                negative[field] = json!(-1.0);
                rejects(McpServer::parse_pad(&negative), "greater than 0");
            }
        }

        #[test]
        fn parse_pad_rejects_unknown_shape_and_layer_names() {
            let mut shape = pad_json();
            shape["shape"] = json!("trapezoid");
            rejects(McpServer::parse_pad(&shape), "invalid shape");

            let mut layer = pad_json();
            layer["layer"] = json!("Layer 47");
            rejects(McpServer::parse_pad(&layer), "invalid layer");
        }

        #[test]
        fn parse_pad_defaults_the_layer_from_whether_it_is_drilled() {
            use crate::altium::pcblib::Layer;

            // No hole: an SMD land, which lives on the top layer.
            let smd = McpServer::parse_pad(&pad_json()).expect("smd pad should parse");
            assert_eq!(smd.layer, Layer::TopLayer);

            // A drilled pad spans the stack, so it defaults to multi-layer.
            let mut drilled = pad_json();
            drilled["hole_size"] = json!(0.8);
            let through = McpServer::parse_pad(&drilled).expect("through pad should parse");
            assert_eq!(through.layer, Layer::MultiLayer);

            // A zero hole is not a hole, so it stays an SMD land.
            let mut zero_hole = pad_json();
            zero_hole["hole_size"] = json!(0.0);
            let still_smd = McpServer::parse_pad(&zero_hole).expect("pad should parse");
            assert_eq!(still_smd.layer, Layer::TopLayer);
        }

        #[test]
        fn parse_pad_reads_hole_shape_and_refuses_an_unknown_one() {
            use crate::altium::pcblib::HoleShape;

            let parsed = |s: &str| {
                let mut json = pad_json();
                json["hole_size"] = json!(0.8);
                json["hole_shape"] = json!(s);
                McpServer::parse_pad(&json).map(|pad| pad.hole_shape)
            };
            let shape = |s: &str| parsed(s).expect("pad should parse");
            assert_eq!(shape("square"), HoleShape::Square);
            assert_eq!(shape("slot"), HoleShape::Slot);
            assert_eq!(shape("round"), HoleShape::Round);
            // Case and separators do not matter, and `circle` is a synonym.
            assert_eq!(shape("SQUARE"), HoleShape::Square);
            assert_eq!(shape("circle"), HoleShape::Round);
            // An unrecognised name is refused, naming the field and the
            // accepted values — a drilled hole silently turned round is a
            // manufacturing change, not a reading.
            let err = parsed("hexagon").unwrap_err();
            assert!(err.contains("Pad '1' hole_shape 'hexagon'"), "{err}");
            assert!(err.contains("round, square, slot"), "{err}");
        }

        /// A full stack takes 32 entries per array, in any accepted spelling
        /// of a shape or a pair; a top-middle-bottom stack takes 3.
        #[test]
        fn parse_pad_reads_a_full_stack_and_a_top_middle_bottom_stack() {
            use crate::altium::pcblib::PadShape;

            let mut json = pad_json();
            json["stack_mode"] = json!("full_stack");
            let mut shapes = vec![json!("round"); 32];
            shapes[1] = json!("rectangular");
            shapes[2] = json!("Rounded-Rectangle");
            json["per_layer_shapes"] = json!(shapes);
            let mut radii = vec![json!(0); 32];
            radii[2] = json!(35);
            json["per_layer_corner_radii"] = json!(radii);
            let mut offsets = vec![json!({ "x": 0.0, "y": 0.0 }); 32];
            offsets[3] = json!([0.1, -0.2]);
            json["per_layer_offsets"] = json!(offsets);
            let pad = McpServer::parse_pad(&json).expect("full stack should parse");
            let shapes = pad.per_layer_shapes.expect("shapes read");
            assert_eq!(shapes.len(), 32);
            assert_eq!(shapes[1], PadShape::Rectangle);
            assert_eq!(
                shapes[2],
                PadShape::RoundedRectangle,
                "any accepted spelling"
            );
            assert_eq!(pad.per_layer_corner_radii.expect("radii read")[2], 35);
            assert_eq!(pad.per_layer_offsets.expect("offsets read")[3], (0.1, -0.2));

            let mut json = pad_json();
            json["stack_mode"] = json!("top_middle_bottom");
            json["per_layer_sizes"] = json!([
                { "width": 1.6, "height": 1.2 },
                [1.4, 1.0],
                { "x": 1.8, "y": 1.4 },
            ]);
            json["per_layer_shapes"] = json!(["round", "rectangle", "octagonal"]);
            let pad = McpServer::parse_pad(&json).expect("top-middle-bottom should parse");
            assert_eq!(
                pad.per_layer_sizes.expect("sizes read"),
                vec![(1.6, 1.2), (1.4, 1.0), (1.8, 1.4)]
            );
            assert_eq!(
                pad.per_layer_shapes.expect("shapes read"),
                vec![PadShape::Round, PadShape::Rectangle, PadShape::Octagonal]
            );
        }

        /// Every way a per-layer array can disagree with what the record
        /// stores is refused, naming the pad, the array and the entry: the
        /// writer would otherwise fill a missing layer from the main size,
        /// ignore an extra one, and the parser used to read an unknown shape
        /// as round, an out-of-range radius as none and a malformed pair as
        /// a zero-size layer.
        #[test]
        fn parse_pad_refuses_a_stack_the_record_cannot_store() {
            let stacked = |mode: &str, key: &str, value: serde_json::Value| {
                let mut json = pad_json();
                json["stack_mode"] = json!(mode);
                json[key] = value;
                McpServer::parse_pad(&json).unwrap_err()
            };
            let full = |entry: serde_json::Value, at: usize, fill: serde_json::Value| {
                let mut entries = vec![fill; 32];
                entries[at] = entry;
                json!(entries)
            };
            let cases = [
                (
                    stacked("full_stack", "per_layer_shapes", full(json!("not-a-shape"), 2, json!("round"))),
                    "Pad '1' per_layer_shapes[2] 'not-a-shape' is not a shape. Valid shapes are",
                ),
                (
                    stacked("full_stack", "per_layer_shapes", full(json!(7), 0, json!("round"))),
                    "Pad '1' per_layer_shapes[0] must be a string, got 7",
                ),
                (
                    stacked("full_stack", "per_layer_corner_radii", full(json!(300), 2, json!(0))),
                    "Pad '1' per_layer_corner_radii[2] must be a whole number from 0 to 100, got 300",
                ),
                (
                    stacked(
                        "full_stack",
                        "per_layer_sizes",
                        full(json!({ "width": 1.0 }), 1, json!({ "width": 1.0, "height": 1.0 })),
                    ),
                    "Pad '1' per_layer_sizes[1] must be {width, height} (or [width, height]), got {\"width\":1.0}",
                ),
                (
                    stacked("full_stack", "per_layer_sizes", json!(vec![json!([1.0, 1.0]); 4])),
                    "Pad '1' per_layer_sizes has 4 entries; full_stack takes 32 (index 0 = Top, 1 = Bottom, 2-31 = Mid layers)",
                ),
                (
                    stacked("top_middle_bottom", "per_layer_shapes", json!(["round", "round"])),
                    "Pad '1' per_layer_shapes has 2 entries; top_middle_bottom takes 3 ([top, mid, bottom])",
                ),
                (
                    stacked("top_middle_bottom", "per_layer_offsets", json!([[0, 0], [0, 0], [0, 0]])),
                    "Pad '1' per_layer_offsets applies to a full_stack pad only",
                ),
                (
                    stacked("top_middle_bottom", "per_layer_corner_radii", json!([0, 0, 0])),
                    "Pad '1' per_layer_corner_radii applies to a full_stack pad only",
                ),
                (
                    stacked("simple", "per_layer_sizes", json!([[1.0, 1.0], [1.0, 1.0], [1.0, 1.0]])),
                    "Pad '1' per_layer_sizes is given but stack_mode is simple",
                ),
                (
                    stacked("full_stack", "per_layer_shapes", json!("round")),
                    "Pad '1' per_layer_shapes must be an array, got \"round\"",
                ),
            ];
            for (err, expected) in cases {
                assert!(err.contains(expected), "expected {expected:?}, got {err:?}");
            }

            // The pad's own corner radius is held to the same range.
            for value in [json!(150), json!(-5), json!(12.5)] {
                let mut json = pad_json();
                json["corner_radius_percent"] = value.clone();
                let err = McpServer::parse_pad(&json).unwrap_err();
                let expected = format!(
                    "Pad '1' corner_radius_percent must be a whole number from 0 to 100, got {value}"
                );
                assert!(
                    err.contains(&expected),
                    "expected {expected:?}, got {err:?}"
                );
            }
            let mut json = pad_json();
            json["corner_radius_percent"] = json!(100);
            assert_eq!(
                McpServer::parse_pad(&json).unwrap().corner_radius_percent,
                Some(100)
            );
        }

        // ---- parse_track / parse_arc / parse_fill -----------------------------

        #[test]
        fn parse_track_names_the_missing_field_and_bad_layer() {
            let base = json!({ "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 1.0, "width": 0.2 });
            for field in ["x1", "y1", "x2", "y2", "width"] {
                let mut json = base.clone();
                json.as_object_mut().unwrap().remove(field);
                rejects(McpServer::parse_track(&json), field);
            }

            let mut bad_layer = base;
            bad_layer["layer"] = json!("Nowhere");
            rejects(McpServer::parse_track(&bad_layer), "invalid layer");
        }

        #[test]
        fn parse_arc_names_the_missing_field_and_bad_layer() {
            let base = json!({
                "x": 0.0, "y": 0.0, "radius": 1.0,
                "start_angle": 0.0, "end_angle": 90.0, "width": 0.2,
            });
            for field in ["x", "y", "radius", "start_angle", "end_angle", "width"] {
                let mut json = base.clone();
                json.as_object_mut().unwrap().remove(field);
                rejects(McpServer::parse_arc(&json), field);
            }

            let mut bad_layer = base;
            bad_layer["layer"] = json!("Nowhere");
            rejects(McpServer::parse_arc(&bad_layer), "invalid layer");
        }

        #[test]
        fn parse_fill_names_the_missing_field_bad_layer_and_reads_rotation() {
            let base = json!({ "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 1.0 });
            for field in ["x1", "y1", "x2", "y2"] {
                let mut json = base.clone();
                json.as_object_mut().unwrap().remove(field);
                rejects(McpServer::parse_fill(&json), field);
            }

            let mut bad_layer = base.clone();
            bad_layer["layer"] = json!("Nowhere");
            rejects(McpServer::parse_fill(&bad_layer), "invalid layer");

            let mut rotated = base;
            rotated["rotation"] = json!(45.0);
            let fill = McpServer::parse_fill(&rotated).expect("fill should parse");
            assert!((fill.rotation - 45.0).abs() < f64::EPSILON);
        }

        // ---- parse_via --------------------------------------------------------

        #[test]
        fn parse_via_names_the_missing_field() {
            let base = json!({ "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3 });
            for field in ["x", "y", "diameter", "hole_size"] {
                let mut json = base.clone();
                json.as_object_mut().unwrap().remove(field);
                rejects(McpServer::parse_via(&json), field);
            }
        }

        #[test]
        fn parse_via_rejects_a_hole_that_cannot_fit_inside_the_ring() {
            let base = json!({ "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3 });

            let mut no_hole = base.clone();
            no_hole["hole_size"] = json!(0.0);
            rejects(McpServer::parse_via(&no_hole), "greater than 0");

            // A hole at or past the outer diameter leaves no annular ring, so
            // the via would be a bare drill with no copper to connect to.
            let mut swallowed = base;
            swallowed["hole_size"] = json!(0.6);
            rejects(McpServer::parse_via(&swallowed), "smaller than diameter");
        }

        #[test]
        fn parse_via_rejects_unknown_layer_names_on_either_end() {
            let base = json!({ "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3 });
            for field in ["from_layer", "to_layer"] {
                let mut json = base.clone();
                json[field] = json!("Nowhere");
                rejects(McpServer::parse_via(&json), field);
            }
        }

        #[test]
        fn parse_via_reads_its_optional_tail() {
            use crate::altium::pcblib::{
                DrillLayerPairType, Layer, MaskExpansionMode, PowerPlaneConnectStyle,
            };

            let via = McpServer::parse_via(&json!({
                "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3,
                "from_layer": "Top Layer", "to_layer": "Mid-Layer 1",
                "solder_mask_expansion": 0.05,
                "solder_mask_expansion_from_hole_edge": true,
                "solder_mask_expansion_mode": "manual",
                "drill_layer_pair_type": "mid",
                "thermal_relief_gap": 0.25,
                "thermal_relief_conductors": 4,
                "thermal_relief_width": 0.3,
                "power_plane_connect_style": "direct",
                "power_plane_relief_expansion": 0.4,
                "power_plane_clearance": 0.5,
                "paste_mask_expansion": 0.06,
                "net_index": 7,
                "hole_positive_tolerance": 0.02,
                "hole_negative_tolerance": 0.01,
            }))
            .expect("via should parse");

            assert_eq!(via.from_layer, Layer::TopLayer);
            assert_eq!(via.to_layer, Layer::MidLayer1);
            assert!(via.solder_mask_expansion_from_hole_edge);
            assert_eq!(via.solder_mask_expansion_mode, MaskExpansionMode::Manual);
            assert_eq!(via.drill_layer_pair_type, DrillLayerPairType::Mid);
            assert_eq!(via.thermal_relief_conductors, 4);
            assert_eq!(
                via.power_plane_connect_style,
                PowerPlaneConnectStyle::Direct
            );
            assert_eq!(via.net_index, 7);
            assert_eq!(via.hole_positive_tolerance, Some(0.02));
            assert_eq!(via.hole_negative_tolerance, Some(0.01));
        }

        #[test]
        fn parse_via_refuses_an_unrecognised_enum_name() {
            // A typo in any of the via's enum fields is refused by name; the
            // via would otherwise be written with the plain default and the
            // caller told nothing.
            let with = |key: &str, value: &str| {
                let mut json = json!({
                    "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3,
                });
                json[key] = json!(value);
                McpServer::parse_via(&json).unwrap_err()
            };
            for (key, accepted) in [
                ("solder_mask_expansion_mode", "none, manual, from_rule"),
                (
                    "drill_layer_pair_type",
                    "through, blind_buried_start, mid, end",
                ),
                ("power_plane_connect_style", "relief, direct, no_connect"),
                (
                    "diameter_stack_mode",
                    "simple, top_middle_bottom, full_stack",
                ),
            ] {
                let err = with(key, "whatever");
                assert!(err.contains(&format!("Via {key} 'whatever'")), "{err}");
                assert!(err.contains(accepted), "{err}");
            }
        }

        // ---- parse_region -----------------------------------------------------

        #[test]
        fn parse_region_needs_three_whole_vertices() {
            let vertex = |x: f64, y: f64| json!({ "x": x, "y": y });
            let err = McpServer::parse_region(&json!({
                "layer": "Top Layer",
                "vertices": [vertex(0.0, 0.0), vertex(1.0, 0.0)],
            }))
            .unwrap_err();
            assert!(err.contains("at least 3 vertices, got 2"), "{err}");

            // A vertex missing a coordinate is refused by name — dropping it
            // would quietly reshape the outline.
            let err = McpServer::parse_region(&json!({
                "layer": "Top Layer",
                "vertices": [vertex(0.0, 0.0), vertex(1.0, 0.0), { "x": 1.0 }],
            }))
            .unwrap_err();
            assert!(
                err.contains("outline vertex 2 is missing a numeric 'y'"),
                "{err}"
            );

            let err = McpServer::parse_region(&json!({ "layer": "Top Layer" })).unwrap_err();
            assert!(err.contains("missing its vertices array"), "{err}");
        }

        #[test]
        fn parse_region_accepts_a_kind_by_name_or_by_id() {
            use crate::altium::pcblib::RegionKind;

            let with_kind = |kind: serde_json::Value| {
                let mut json = json!({
                    "layer": "Top Layer",
                    "vertices": [
                        { "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 0.0 }, { "x": 0.0, "y": 1.0 },
                    ],
                });
                json["kind"] = kind;
                McpServer::parse_region(&json)
                    .expect("region should parse")
                    .kind
            };

            assert_eq!(with_kind(json!("cutout")), RegionKind::Cutout);
            assert_eq!(with_kind(json!("copper")), RegionKind::Copper);
            assert_eq!(with_kind(json!("named_region")), RegionKind::NamedRegion);
            assert_eq!(with_kind(json!("cavity")), RegionKind::Cavity);
            // A numeric KIND, as a string and as a number.
            assert_eq!(with_kind(json!("4")), RegionKind::Cavity);
            assert_eq!(with_kind(json!(4)), RegionKind::Cavity);
            // An unrecognised name is refused rather than read as copper.
            let mut json = json!({
                "layer": "Top Layer",
                "vertices": [
                    { "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 0.0 }, { "x": 0.0, "y": 1.0 },
                ],
            });
            json["kind"] = json!("nonsense");
            let err = McpServer::parse_region(&json).unwrap_err();
            assert!(err.contains("Region kind 'nonsense'"), "{err}");
            assert!(
                err.contains("copper, cutout, named_region, cavity"),
                "{err}"
            );
        }

        #[test]
        fn parse_region_reads_the_name_cavity_and_hole_contours() {
            let region = McpServer::parse_region(&json!({
                "layer": "Top Layer",
                "vertices": [
                    { "x": -1.0, "y": -1.0 }, { "x": 1.0, "y": -1.0 },
                    { "x": 1.0, "y": 1.0 }, { "x": -1.0, "y": 1.0 },
                ],
                "name": "CAVITY_A",
                "cavity_height": 1.5,
                "sub_poly_index": 2,
                "union_index": 3,
                "is_shape_based": true,
                "holes": [[
                    { "x": -0.5, "y": -0.5 }, { "x": 0.5, "y": -0.5 }, { "x": 0.0, "y": 0.5 },
                ]],
            }))
            .expect("region should parse");

            assert_eq!(region.name, "CAVITY_A");
            assert!((region.cavity_height - 1.5).abs() < f64::EPSILON);
            assert_eq!(region.sub_poly_index, 2);
            assert_eq!(region.union_index, 3);
            assert!(region.is_shape_based);
            assert_eq!(region.holes.len(), 1);
            assert_eq!(region.holes[0].len(), 3);
        }

        // ---- SchLib shape helpers ---------------------------------------------

        #[test]
        fn polylines_and_polygons_need_enough_points_to_draw() {
            let point = |x: f64, y: f64| json!({ "x": x, "y": y });

            // A polyline is a run of segments, so one point draws nothing.
            assert!(
                McpServer::parse_schlib_polyline(&json!({ "points": [point(0.0, 0.0)] })).is_none()
            );
            assert!(McpServer::parse_schlib_polyline(
                &json!({ "points": [point(0.0, 0.0), point(1.0, 1.0)] })
            )
            .is_some());

            // A polygon is a closed area, so it needs a third point.
            assert!(McpServer::parse_schlib_polygon(
                &json!({ "points": [point(0.0, 0.0), point(1.0, 0.0)] })
            )
            .is_none());
            assert!(McpServer::parse_schlib_polygon(&json!({
                "points": [point(0.0, 0.0), point(1.0, 0.0), point(0.0, 1.0)],
            }))
            .is_some());
        }

        #[test]
        fn an_image_with_undecodable_data_still_parses_without_it() {
            // The bytes are optional decoration; a corrupt payload must not
            // take the whole symbol down with it.
            let image = McpServer::parse_schlib_image(&json!({
                "x1": 0.0, "y1": 0.0, "x2": 10.0, "y2": 10.0,
                "filename": "logo.bmp",
                "image_data": "!!! not base64 !!!",
            }))
            .expect("image should parse");
            assert!(image.image_data.is_none());
        }

        #[test]
        fn label_justification_accepts_the_spelling_variants() {
            // Both spellings of centre and either separator land on the same
            // anchor, for a schematic label and a PcbLib text alike.
            use crate::altium::schlib::TextJustification;

            let justified = |s: &str| {
                McpServer::parse_schlib_label(&json!({
                    "x": 0.0, "y": 0.0, "text": "REF", "justification": s,
                }))
                .expect("label should parse")
                .justification
            };

            // Both spellings of centre, with and without separators.
            assert_eq!(justified("middle-center"), TextJustification::MiddleCenter);
            assert_eq!(justified("middle_centre"), TextJustification::MiddleCenter);
            assert_eq!(justified("center"), TextJustification::MiddleCenter);
            assert_eq!(justified("bottom-centre"), TextJustification::BottomCenter);
            assert_eq!(justified("bottomright"), TextJustification::BottomRight);
            assert_eq!(justified("center-left"), TextJustification::MiddleLeft);
            assert_eq!(justified("centre-right"), TextJustification::MiddleRight);
            assert_eq!(justified("topleft"), TextJustification::TopLeft);
            assert_eq!(justified("top-centre"), TextJustification::TopCenter);
            // An unrecognised anchor is refused, not quietly moved to a
            // corner the caller did not ask for.
            let err = McpServer::parse_schlib_label(&json!({
                "x": 0.0, "y": 0.0, "text": "REF", "justification": "sideways",
            }))
            .unwrap_err();
            assert!(err.contains("Label justification 'sideways'"), "{err}");
            assert!(err.contains("bottom_left, bottom_center"), "{err}");
        }

        /// Every enum-valued pad field refuses an unrecognised name, naming
        /// the pad and the field; a non-string value is refused too.
        #[test]
        fn parse_pad_refuses_an_unrecognised_enum_name() {
            let with = |key: &str, value: serde_json::Value| {
                let mut json = pad_json();
                json[key] = value;
                McpServer::parse_pad(&json).unwrap_err()
            };
            for (key, accepted) in [
                ("paste_mask_expansion_mode", "none, manual, from_rule"),
                ("solder_mask_expansion_mode", "none, manual, from_rule"),
                ("power_plane_connect_style", "relief, direct, no_connect"),
                ("stack_mode", "simple, top_middle_bottom, full_stack"),
            ] {
                let err = with(key, json!("whatever"));
                assert!(err.contains(&format!("Pad '1' {key} 'whatever'")), "{err}");
                assert!(err.contains(accepted), "{err}");
            }
            let err = with("hole_shape", json!(5));
            assert!(err.contains("hole_shape must be a string, got 5"), "{err}");
            assert!(err.contains("round, square, slot"), "{err}");
        }

        /// A font name is held to a Windows face name's 31 UTF-16 units — the
        /// record's field holds no more — rather than written cut short.
        #[test]
        fn parse_text_refuses_a_font_name_longer_than_a_face_name() {
            let with = |key: &str, name: String| {
                let mut json = json!({ "x": 0.0, "y": 0.0, "text": "T", "height": 1.0 });
                json[key] = json!(name);
                McpServer::parse_text(&json)
            };
            let longest = "F".repeat(31);
            assert_eq!(
                with("font_name", longest.clone()).unwrap().font_name,
                longest
            );
            assert_eq!(
                with("barcode_font_name", longest.clone())
                    .unwrap()
                    .barcode_font_name,
                longest
            );
            let err = with("font_name", "F".repeat(32)).unwrap_err();
            assert!(
                err.contains(
                    "Text font_name 'FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF' is 32 UTF-16 units long"
                ),
                "{err}"
            );
            // Units, not chars: a surrogate pair counts twice.
            let err = with("barcode_font_name", "\u{1F600}".repeat(16)).unwrap_err();
            assert!(err.contains("is 32 UTF-16 units long"), "{err}");
        }

        /// A `PcbLib` text's kind, stroke font and justification refuse an
        /// unrecognised name each.
        #[test]
        fn parse_text_refuses_an_unrecognised_enum_name() {
            let with = |key: &str, value: &str| {
                let mut json = json!({ "x": 0.0, "y": 0.0, "text": "T", "height": 1.0 });
                json[key] = json!(value);
                McpServer::parse_text(&json).unwrap_err()
            };
            let err = with("kind", "hologram");
            assert!(
                err.contains("Text kind 'hologram'") && err.contains("stroke, true_type, bar_code"),
                "{err}"
            );
            let err = with("stroke_font", "comic");
            assert!(
                err.contains("Text stroke_font 'comic'")
                    && err.contains("default, sans_serif, serif"),
                "{err}"
            );
            let err = with("justification", "sideways");
            assert!(
                err.contains("Text justification 'sideways'") && err.contains("bottom_left"),
                "{err}"
            );
        }

        /// A pin's orientation, electrical type and decorations refuse an
        /// unrecognised name each, naming the pin.
        #[test]
        fn parse_schlib_pin_refuses_an_unrecognised_enum_name() {
            let with = |key: &str, value: &str| {
                let mut json = json!({ "designator": "7", "x": 0, "y": 0 });
                json[key] = json!(value);
                McpServer::parse_schlib_pin(&json).unwrap_err()
            };
            let err = with("orientation", "sideways");
            assert!(
                err.contains("Pin '7' orientation 'sideways'")
                    && err.contains("left, right, up, down"),
                "{err}"
            );
            let err = with("electrical_type", "magic");
            assert!(
                err.contains("Pin '7' electrical_type 'magic'") && err.contains("open_collector"),
                "{err}"
            );
            let err = with("symbol_inner_edge", "sparkle");
            assert!(
                err.contains("Pin '7' symbol_inner_edge 'sparkle'") && err.contains("clock"),
                "{err}"
            );
        }

        /// A region's holes are refused by name when malformed, its layer
        /// when unknown, and its kind takes the serde object form a read
        /// echoes for a non-standard KIND.
        #[test]
        fn parse_region_refuses_malformed_holes_and_layers_and_reads_the_other_kind() {
            use crate::altium::pcblib::RegionKind;
            let base = || {
                json!({
                    "layer": "Top Layer",
                    "vertices": [
                        { "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 0.0 }, { "x": 0.0, "y": 1.0 },
                    ],
                })
            };
            let with = |key: &str, value: serde_json::Value| {
                let mut json = base();
                json[key] = value;
                McpServer::parse_region(&json)
            };

            let err = with("holes", json!("nope")).unwrap_err();
            assert!(err.contains("holes must be an array"), "{err}");
            let err = with("holes", json!(["nope"])).unwrap_err();
            assert!(err.contains("hole 0 must be an array of vertices"), "{err}");
            let err = with(
                "holes",
                json!([[{ "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 0.0 }]]),
            )
            .unwrap_err();
            assert!(
                err.contains("hole 0 needs at least 3 vertices, got 2"),
                "{err}"
            );
            let err = with(
                "holes",
                json!([[{ "x": 0.0, "y": 0.0 }, { "x": 1.0 }, { "x": 0.0, "y": 1.0 }]]),
            )
            .unwrap_err();
            assert!(
                err.contains("hole 0 vertex 1 is missing a numeric 'y'"),
                "{err}"
            );

            let err = with("layer", json!("Nowhere")).unwrap_err();
            assert!(err.contains("Region has invalid layer 'Nowhere'"), "{err}");

            assert_eq!(
                with("kind", json!({ "other": 7 })).unwrap().kind,
                RegionKind::Other(7)
            );
            let err = with("kind", json!({ "bogus": 1 })).unwrap_err();
            assert!(
                err.contains("Region kind") && err.contains("not recognised"),
                "{err}"
            );
            let err = with("kind", json!(4.5)).unwrap_err();
            assert!(err.contains("is not a KIND integer"), "{err}");
        }

        /// A track or arc without a layer lands on the overlay, the layer a
        /// silkscreen outline is drawn on.
        #[test]
        fn a_track_or_arc_without_a_layer_defaults_to_the_top_overlay() {
            use crate::altium::pcblib::Layer;
            let track = McpServer::parse_track(&json!({
                "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.2,
            }))
            .expect("track should parse");
            assert_eq!(track.layer, Layer::TopOverlay);
            let arc = McpServer::parse_arc(&json!({
                "x": 0.0, "y": 0.0, "radius": 1.0, "start_angle": 0.0, "end_angle": 90.0,
                "width": 0.2,
            }))
            .expect("arc should parse");
            assert_eq!(arc.layer, Layer::TopOverlay);
        }

        /// Every value the schemas advertise parses, and every synonym lands
        /// on a name that does: a list entry that no longer matches its enum
        /// would advertise a value the parser then refuses.
        #[test]
        fn every_advertised_enum_value_parses() {
            use crate::altium::pcblib::{
                DrillLayerPairType, HoleShape, MaskExpansionMode, PadStackMode,
                PowerPlaneConnectStyle, RegionKind, StrokeFont, TextKind,
            };
            use crate::altium::schlib::{
                PinElectricalType, PinOrientation, PinSymbol, TextJustification,
            };
            use crate::mcp::tools::parsing::accepted::*;

            /// Checks one list against the enum it names, then its synonyms.
            fn check<T: serde::de::DeserializeOwned>(
                names: &[&str],
                synonyms: &[(&str, &str)],
                what: &str,
            ) {
                assert!(!names.is_empty(), "{what}: no accepted values");
                for name in names {
                    super::super::parse_enum::<T>(name, what, names, synonyms)
                        .unwrap_or_else(|e| panic!("{what} '{name}': {e}"));
                }
                for (synonym, name) in synonyms {
                    assert!(
                        names.contains(name),
                        "{what}: synonym '{synonym}' points at '{name}', which is not accepted"
                    );
                    super::super::parse_enum::<T>(synonym, what, names, synonyms)
                        .unwrap_or_else(|e| panic!("{what} synonym '{synonym}': {e}"));
                }
            }

            check::<HoleShape>(HOLE_SHAPES, HOLE_SHAPE_SYNONYMS, "hole_shape");
            check::<MaskExpansionMode>(MASK_EXPANSION_MODES, &[], "mask_expansion_mode");
            check::<PowerPlaneConnectStyle>(
                POWER_PLANE_CONNECT_STYLES,
                &[],
                "power_plane_connect_style",
            );
            check::<PadStackMode>(STACK_MODES, &[], "stack_mode");
            check::<DrillLayerPairType>(DRILL_LAYER_PAIR_TYPES, &[], "drill_layer_pair_type");
            check::<TextKind>(TEXT_KINDS, &[], "kind");
            check::<StrokeFont>(STROKE_FONTS, &[], "stroke_font");
            check::<RegionKind>(REGION_KINDS, &[], "region kind");
            check::<TextJustification>(
                TEXT_JUSTIFICATIONS,
                TEXT_JUSTIFICATION_SYNONYMS,
                "justification",
            );
            check::<PinOrientation>(PIN_ORIENTATIONS, &[], "orientation");
            check::<PinElectricalType>(
                PIN_ELECTRICAL_TYPES,
                PIN_ELECTRICAL_TYPE_SYNONYMS,
                "electrical_type",
            );
            check::<PinSymbol>(PIN_SYMBOLS, PIN_SYMBOL_SYNONYMS, "symbol");
        }
    }

    /// A GUID is kept in whatever spelling of its 32 hex digits it came in;
    /// anything else is refused by record and key, since the writer could
    /// only drop the identity or invent one in its place.
    #[test]
    fn every_guid_field_is_held_to_the_guid_form() {
        let base = json!({ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 });
        for guid in [
            "{01234567-89AB-CDEF-0123-456789ABCDEF}",
            "01234567-89ab-cdef-0123-456789abcdef",
            "0123456789ABCDEF0123456789ABCDEF",
        ] {
            let mut json = base.clone();
            json["guid"] = json!(guid);
            json["identity_guid"] = json!(guid);
            json["identity_guid_b"] = json!(guid);
            let pad = McpServer::parse_pad(&json).expect("a GUID in any spelling");
            assert_eq!(pad.guid.as_deref(), Some(guid), "kept verbatim");
            assert_eq!(pad.identity_guid.as_deref(), Some(guid));
            assert_eq!(pad.identity_guid_b.as_deref(), Some(guid));
        }

        let bad = "not-a-guid";
        let refused = |err: String, field: &str| {
            let expected =
                format!("{field} '{bad}' is not a GUID ({{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}})");
            assert!(
                err.contains(&expected),
                "expected {expected:?}, got {err:?}"
            );
        };
        for key in ["guid", "identity_guid", "identity_guid_b"] {
            let mut json = base.clone();
            json[key] = json!(bad);
            refused(
                McpServer::parse_pad(&json).unwrap_err(),
                &format!("Pad '1' {key}"),
            );
        }
        refused(
            McpServer::parse_track(&json!({
                "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.2, "guid": bad,
            }))
            .unwrap_err(),
            "Track guid",
        );
        refused(
            McpServer::parse_arc(&json!({
                "x": 0.0, "y": 0.0, "radius": 1.0, "start_angle": 0.0, "end_angle": 90.0,
                "width": 0.2, "guid": bad,
            }))
            .unwrap_err(),
            "Arc guid",
        );
        refused(
            McpServer::parse_fill(&json!({
                "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 1.0, "guid": bad,
            }))
            .unwrap_err(),
            "Fill guid",
        );
        refused(
            McpServer::parse_via(&json!({
                "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3, "guid": bad,
            }))
            .unwrap_err(),
            "Via guid",
        );
        refused(
            McpServer::parse_text(&json!({
                "x": 0.0, "y": 0.0, "text": "T", "height": 1.0, "guid": bad,
            }))
            .unwrap_err(),
            "Text guid",
        );
        refused(
            McpServer::parse_region(&json!({
                "layer": "Top Layer",
                "vertices": [{ "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 0.0 }, { "x": 0.0, "y": 1.0 }],
                "guid": bad,
            }))
            .unwrap_err(),
            "Region guid",
        );
        refused(
            McpServer::parse_component_body_json(&json!({ "overall_height": 1.0, "guid": bad }))
                .unwrap_err(),
            "Component body guid",
        );
    }

    /// A component body on a layer the model does not know is refused like
    /// every other primitive, not quietly placed on Top 3D Body.
    #[test]
    fn parse_component_body_refuses_an_unknown_layer() {
        let err = McpServer::parse_component_body_json(&json!({
            "overall_height": 1.0, "layer": "Nowhere",
        }))
        .unwrap_err();
        assert!(
            err.contains("Component body has invalid layer 'Nowhere'"),
            "{err}"
        );
        let body = McpServer::parse_component_body_json(&json!({ "overall_height": 1.0 }))
            .expect("no layer given");
        assert_eq!(body.layer, crate::altium::pcblib::Layer::Top3DBody);
    }
    /// A `top_middle_bottom` stack has no per-layer corner-radius bytes,
    /// so a `rounded_rectangle` slot cannot be stored: it is refused by
    /// rule rather than written and read back as plain round.
    #[test]
    fn parse_pad_refuses_rounded_rectangle_in_a_top_middle_bottom_stack() {
        let json = json!({
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.6, "height": 1.6,
            "hole_size": 0.8,
            "stack_mode": "top_middle_bottom",
            "per_layer_sizes": [[1.6, 1.6], [1.4, 1.4], [1.6, 1.6]],
            "per_layer_shapes": ["round", "rounded_rectangle", "rectangle"],
        });
        let err = McpServer::parse_pad(&json).unwrap_err();
        assert!(
            err.contains("per_layer_shapes[1]: rounded_rectangle needs a per-layer corner radius"),
            "{err}"
        );
        assert!(err.contains("full_stack"), "{err}");
    }
}
