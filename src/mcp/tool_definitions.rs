//! Tool definitions for the MCP `tools/list` response.
//!
//! Extracted from `server.rs` to keep that file navigable. This is purely the
//! static tool schema (names, descriptions, JSON input schemas); it carries no
//! behaviour, so it lives apart from the request handlers.

use serde_json::json;

use crate::mcp::server::{McpServer, ToolAnnotations, ToolDefinition};
use crate::mcp::tools::{accepted, UPDATE_PRIMITIVE_KINDS};

/// The display name of each tool — the MCP `title` a client shows in place of
/// the identifier.
fn tool_title(name: &str) -> &'static str {
    match name {
        "read_pcblib" => "Read a PcbLib",
        "write_pcblib" => "Write a PcbLib",
        "read_schlib" => "Read a SchLib",
        "write_schlib" => "Write a SchLib",
        "list_components" => "List components",
        "get_component" => "Get a component",
        "search_components" => "Search components",
        "component_exists" => "Check components exist",
        "render_footprint" => "Preview a footprint",
        "render_symbol" => "Preview a symbol",
        "extract_style" => "Extract a library's style",
        "diff_libraries" => "Compare two libraries",
        "compare_components" => "Compare two components",
        "update_component" => "Update a component in place",
        "update_pad" => "Update a pad",
        "update_primitive" => "Update a primitive",
        "batch_update" => "Batch-update a library",
        "reorder_components" => "Reorder components",
        "manage_schlib_parameters" => "Manage symbol parameters",
        "manage_schlib_footprints" => "Manage symbol footprint links",
        "delete_component" => "Delete components",
        "copy_component" => "Duplicate a component",
        "rename_component" => "Rename a component",
        "copy_component_cross_library" => "Copy a component to another library",
        "bulk_rename" => "Rename components by pattern",
        "merge_libraries" => "Merge libraries",
        "write_libpkg" => "Write a LibPkg project",
        "export_library" => "Export a library",
        "import_library" => "Import a library",
        "validate_library" => "Validate a library",
        "repair_library" => "Repair a library",
        "extract_step_model" => "Extract a STEP model",
        "list_step_models" => "List STEP models",
        "list_backups" => "List backups",
        "restore_backup" => "Restore a backup",
        other => panic!("tool {other} has no title"),
    }
}

/// Fills in each tool's `title` and `annotations`: the hints derive from the
/// same mutating-tool classification the rate limiter and audit log use, so a
/// client is told exactly which tools change files.
fn annotate(mut tools: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
    for tool in &mut tools {
        let mutating = McpServer::is_mutating_tool(&tool.name);
        let title = tool_title(&tool.name).to_string();
        tool.annotations = Some(ToolAnnotations {
            title: title.clone(),
            read_only_hint: !mutating,
            destructive_hint: mutating,
            open_world_hint: false,
        });
        tool.title = Some(title);
    }
    tools
}

impl McpServer {
    /// Returns the list of available tools.
    ///
    /// These are low-level file I/O and primitive placement tools.
    /// The AI handles IPC calculations and design decisions.
    ///
    /// Built from one helper per tool family: the `json!` schema literals
    /// allocate their temporaries on the stack, and a single function holding
    /// all of them breaches clippy's `large_stack_frames` threshold.
    pub(crate) fn get_tool_definitions() -> Vec<ToolDefinition> {
        let mut tools = Self::reading_tool_definitions();
        tools.extend(Self::style_tool_definitions());
        tools.extend(Self::writing_tool_definitions());
        tools.extend(Self::management_tool_definitions());
        annotate(tools)
    }

    /// Tool schemas for the library-reading family.
    #[allow(clippy::too_many_lines)]
    fn reading_tool_definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "read_pcblib".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "read_pcblib", "arguments": {"filepath": "./MyLibrary.PcbLib"}})),
                description: Some(
                    "Read an Altium .PcbLib file and return its contents including footprints \
                     with their primitives (pads, vias, tracks, arcs, regions, fills, text, \
                     component_bodies). Returns structured data that can be used to understand \
                     existing footprint styles. All coordinates and dimensions are in millimetres \
                     (mm). Fields such as guid, unique_id, raw_tail, raw_block, raw_geometry, \
                     raw_layer_id, param_key_order, primitive_order and storage_name are \
                     fidelity carriers: \
                     pass them back unchanged to write_pcblib or update_component and the \
                     rewrite is byte-identical to the source; omit them when authoring from \
                     scratch. \
                     Each footprint is the same JSON shape get_component, export_library and \
                     write_pcblib use; a list with no entries and an optional field with no \
                     value are omitted rather than empty/null. \
                     For large libraries, use component_name to fetch specific footprints, \
                     or use limit/offset for pagination."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Optional: fetch only this footprint, by name in any case; a name the library does not hold is an error naming the available footprints"
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Optional: maximum number of footprints to return, 1 or more (default: all)"
                        },
                        "offset": {
                            "type": "integer",
                            "minimum": 0,
                            "description": "Optional: skip the first N footprints, 0 or more (default: 0)"
                        },
                        "compact": {
                            "type": "boolean",
                            "description": "If true (default), omit per-layer pad data when stack_mode is Simple. Set to false for full output."
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            ToolDefinition {
                name: "read_schlib".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "read_schlib", "arguments": {"filepath": "./MySymbols.SchLib"}})),
                description: Some(
                    "Read an Altium .SchLib file and return its contents including symbols \
                     with their primitives (pins, rectangles, round_rects, lines, polylines, \
                     polygons, arcs, pies, images, text_frames, beziers, ellipses, \
                     elliptical_arcs, labels, ieee_symbols), parameters and footprint links. \
                     Coordinates are in schematic units (10 units = 1 grid square, not mm). \
                     Fields such as unique_id, primitive_order, header_params, raw_params, \
                     all_pin_count, extra_streams and storage_name are fidelity carriers: pass them back \
                     unchanged to write_schlib or update_component and the rewrite is \
                     byte-identical to the source; omit them when authoring from scratch. \
                     Each symbol is the same JSON shape get_component, \
                     export_library and write_schlib use; a list with no entries and an \
                     optional field with no value are omitted rather than empty/null. \
                     For large libraries, use component_name to fetch specific \
                     symbols, or use limit/offset for pagination."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .SchLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Optional: fetch only this symbol, by name in any case; a name the library does not hold is an error naming the available symbols"
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Optional: maximum number of symbols to return, 1 or more (default: all)"
                        },
                        "offset": {
                            "type": "integer",
                            "minimum": 0,
                            "description": "Optional: skip the first N symbols, 0 or more (default: 0)"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            ToolDefinition {
                name: "list_components".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "list_components", "arguments": {"filepath": "./MyLibrary.PcbLib", "limit": 50, "offset": 0, "include_metadata": true}})),
                description: Some(
                    "List all component/footprint names in an Altium library file (.PcbLib or .SchLib). \
                     Supports pagination with limit/offset for large libraries. Use include_metadata \
                     for additional details like part_count and pin_count."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the library file"
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Maximum number of components to return, 1 or more (optional, default: all)"
                        },
                        "offset": {
                            "type": "integer",
                            "minimum": 0,
                            "description": "Number of components to skip, 0 or more (optional, default: 0)"
                        },
                        "include_metadata": {
                            "type": "boolean",
                            "description": "If true, return objects with metadata instead of just names: a footprint's description, one count per primitive kind and has_3d_model; a symbol's description, designator, part_count, pin_count and footprint_count. Default: false"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
        ]
    }

    /// Tool schemas for the style-extraction family.
    fn style_tool_definitions() -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: "extract_style".to_string(),
            title: None,
            annotations: None,
            example: Some(
                serde_json::json!({"name": "extract_style", "arguments": {"filepath": "./MyLibrary.PcbLib"}}),
            ),
            description: Some(
                "Extract style information from an existing Altium library file. A PcbLib \
                     reports track and arc widths per layer, pad shapes, text heights and the \
                     layer usage of every primitive kind (a via counts as Multi-Layer); a \
                     SchLib reports pin lengths, stroke widths and the stroke, fill and text \
                     colours of every record kind. Use this to learn from existing libraries \
                     and create consistent new components."
                    .to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "filepath": {
                        "type": "string",
                        "description": "Path to the .PcbLib or .SchLib file"
                    }
                },
                "required": ["filepath"]
            }),
        }]
    }

    /// The footprint object `write_pcblib` takes per `footprints` entry and
    /// `update_component` takes as `footprint`: one schema, so a value is
    /// type-checked the same way whichever tool carries it, and every key the
    /// footprint parser accepts is described here (pinned by
    /// `every_accepted_key_is_described_by_the_tool_schema`).
    #[allow(clippy::too_many_lines)]
    fn footprint_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Footprint name (e.g., 'RESC1608X55N')"
                },
                "storage_name": { "type": "string", "description": "The CFB storage the component was read from, as read_pcblib/read_schlib emit it. Pass it back unchanged on a read-modify-write so the component stays where Altium looks for it (Altium re-derives a short name's storage and maps a long one through SectionKeys, so a moved storage is a component it cannot load); omit when authoring or renaming, and the storage name is derived as Altium would: / \\ : ! * become _, then a cut at 31 characters." },
                "description": {
                    "type": "string",
                    "description": "Footprint description. Keep to 256 characters if the library will be imported into an Altium 365 workspace — that importer refuses longer ones; a longer description is written and reported as a validation warning."
                },
                "pads": {
                    "type": "array",
                    "description": "Pad definitions",
                    "items": {
                        "type": "object",
                        "properties": {
                            "designator": { "type": "string" },
                            "x": { "type": "number", "description": "X position in mm" },
                            "y": { "type": "number", "description": "Y position in mm" },
                            "width": { "type": "number", "description": "Pad width in mm" },
                            "height": { "type": "number", "description": "Pad height in mm" },
                            "shape": {
                                "type": "string",
                                "enum": ["rectangle", "rounded_rectangle", "round", "circle", "oval", "octagonal"],
                                "description": "Pad shape. Omitting this field yields rounded_rectangle, which suits chip/QFN lands but is WRONG for BGA/CSP: circular NSMD BGA lands must set \"round\" explicitly. rectangle is conventional for pin 1; round/circle are equivalent and used for both BGA lands and through-hole pads; also oval, octagonal. Matching is case-insensitive and ignores '_'/'-'."
                            },
                            "layer": { "type": "string", "description": "Layer name: Top Layer, Bottom Layer, Multi-Layer (default for SMD)" },
                            "hole_size": { "type": "number", "description": "Hole diameter for through-hole pads (mm)" },
                            "is_plated": { "type": "boolean", "description": "Whether the hole is plated. Altium stores this for every pad (SMD included). Default: true" },
                            "solder_mask_expansion_from_hole_edge": { "type": "boolean", "description": "Measure solder-mask expansion from the HOLE edge instead of the pad edge. Only meaningful on a pad with a hole. Default: false" },
                            "jumper_id": { "type": "integer", "minimum": 0, "maximum": 32767, "description": "Jumper group id. Pads sharing a non-zero id are linked as a jumper / 0-ohm net. Default: 0" },
                            "stack_mode": { "type": "string", "enum": accepted::STACK_MODES, "description": "Per-layer pad stack. \"simple\" (default) uses one size and shape on every layer; \"top_middle_bottom\" takes 3 per-layer entries [top, mid, bottom]; \"full_stack\" takes 32 (index 0 = Top, 1 = Bottom, 2-31 = Mid layers)." },
                            "per_layer_sizes": { "type": "array", "description": "Per-layer pad sizes in mm: exactly 3 entries [top, mid, bottom] for \"top_middle_bottom\", 32 for \"full_stack\" (index 0 = Top, 1 = Bottom, 2-31 = Mid layers); refused on a \"simple\" pad. Any other count is refused, since the file cannot store it.", "items": { "type": "object", "properties": { "width": { "type": "number" }, "height": { "type": "number" } }, "required": ["width", "height"] } },
                            "per_layer_shapes": { "type": "array", "description": "Per-layer pad shapes, same ordering and count as per_layer_sizes (3 or 32). Same vocabulary as shape; an entry that is not a shape is refused, and rounded_rectangle is full_stack-only (a top_middle_bottom stack stores no per-layer corner radius).", "items": { "type": "string", "enum": ["rectangle", "rounded_rectangle", "round", "circle", "oval", "octagonal"] } },
                            "per_layer_corner_radii": { "type": "array", "description": "Per-layer corner radius as a whole-number percentage (0-100) for rounded-rectangle layers; \"full_stack\" only, 32 entries.", "items": { "type": "integer", "minimum": 0, "maximum": 100 } },
                            "per_layer_offsets": { "type": "array", "description": "Per-layer pad offsets in mm from the pad centre; \"full_stack\" only, 32 entries.", "items": { "type": "object", "properties": { "x": { "type": "number" }, "y": { "type": "number" } }, "required": ["x", "y"] } },
                            "hole_shape": {
                                "type": "string",
                                "enum": accepted::HOLE_SHAPES,
                                "description": "Drill hole shape. Default: round. Use slot for oblong holes (set hole_slot_length)"
                            },
                            "hole_slot_length": { "type": "number", "description": "Slot length in mm for a slot hole (hole_shape=slot). Default: 0" },
                            "hole_rotation": { "type": "number", "description": "Hole rotation in degrees (rotates a slot hole). Default: 0" },
                            "hole_positive_tolerance": { "type": "number", "description": "Positive drill tolerance in mm (optional; omit to leave unset)" },
                            "hole_negative_tolerance": { "type": "number", "description": "Negative drill tolerance in mm (optional; omit to leave unset)" },
                            "solder_mask_expansion": { "type": "number", "description": "Solder mask expansion in mm (optional; omit to use the rule default)" },
                            "solder_mask_expansion_mode": {
                                "type": "string",
                                "enum": accepted::MASK_EXPANSION_MODES,
                                "description": "Solder mask expansion mode. 'none' (the default) leaves the cached value stale so Altium takes the expansion from the design rule; 'from_rule' tells Altium the stored value is a rule result to honour as-is; 'manual' uses the stored value as hand-specified."
                            },
                            "paste_mask_expansion": { "type": "number", "description": "Paste (stencil) mask expansion in mm (optional; omit to use the rule default)" },
                            "paste_mask_expansion_mode": {
                                "type": "string",
                                "enum": accepted::MASK_EXPANSION_MODES,
                                "description": "Paste mask expansion mode. 'none' (the default) leaves the cached value stale so Altium takes the expansion from the design rule; 'from_rule' tells Altium the stored value is a rule result to honour as-is; 'manual' uses the stored value as hand-specified."
                            },
                            "corner_radius_percent": { "type": "integer", "minimum": 0, "maximum": 100, "description": "Rounded-rectangle corner radius as a whole-number percentage of the shorter side (0-100; anything else is refused). Default: 0" },
                            "rotation": { "type": "number", "description": "Pad rotation in degrees. Default: 0" },
                            "power_plane_connect_style": {
                                "type": "string",
                                "enum": accepted::POWER_PLANE_CONNECT_STYLES,
                                "description": "How the pad connects to an internal power plane. Default: relief (thermal spokes)"
                            },
                            "relief_conductor_width": { "type": "number", "description": "Thermal-relief spoke (conductor) width in mm. Default: 0.254 (10 mil)" },
                            "relief_entries": { "type": "integer", "minimum": 0, "description": "Number of thermal-relief spokes. Default: 4" },
                            "relief_air_gap": { "type": "number", "description": "Thermal-relief air-gap width in mm. Default: 0.254 (10 mil)" },
                            "power_plane_relief_expansion": { "type": "number", "description": "Power-plane relief expansion in mm. Default: 0.508 (20 mil)" },
                            "power_plane_clearance": { "type": "number", "description": "Power-plane (anti-pad) clearance to the plane in mm. Default: 0.508 (20 mil)" },
                            "net_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Net index into the board net list (common header, 0-65534; 65535 = no net). Normally omitted for library footprints; preserved on a read-modify-write. Default: 65535" },
                            "polygon_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                            "component_index": { "type": "integer", "minimum": -1, "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "identity_guid": { "type": "string", "description": "Per-pad identity GUID (braced string, e.g. \"{A5172B29-...}\"); preserved verbatim on read-modify-write, freshly generated if omitted" },
                            "identity_guid_b": { "type": "string", "description": "Pad-stack/footprint-scoped identity GUID (braced string); preserved verbatim on read-modify-write, freshly generated if omitted" },
                            "flags": { "type": ["string", "integer"], "minimum": 0, "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"LOCKED\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom, 32=testpoint-top, 64=testpoint-bottom; 128 and above are on-disk bits read_pcblib carries verbatim as DISK_BIT_n — pass them back unchanged). Default: none" },
                            "guid": { "type": "string", "description": "Altium's stable identity for the primitive, from the footprint's PrimitiveGuids stream (braced string, e.g. \"{A5172B29-...}\"), as read_pcblib emits it. Pass it back unchanged so a read-modify-write keeps the identity Altium tracks for ECO; omit when authoring (a from-scratch primitive records none)." },
                            "raw_layer_id": { "type": "integer", "minimum": 0, "maximum": 255, "description": "The record's on-disk layer byte (0-255), emitted by read_pcblib only when it maps to no named layer (the primitive then reads as Multi-Layer). Pass it back unchanged so the rewrite keeps the byte; moving the primitive to a named layer discards it. Omit when authoring." },
                            "raw_tail": { "type": "string", "description": "Base64 of the pad record's extended tail exactly as read_pcblib emitted it; the writer overlays the typed fields on it so the rewrite matches the source bytes whichever Altium version wrote them. Pass back unchanged; omit when authoring (the template tail is used)." }
                        },
                        "required": ["designator", "x", "y", "width", "height"]
                    }
                },
                "tracks": {
                    "type": "array",
                    "description": "Track/line definitions for silkscreen, assembly, etc.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x1": { "type": "number" },
                            "y1": { "type": "number" },
                            "x2": { "type": "number" },
                            "y2": { "type": "number" },
                            "width": { "type": "number", "description": "Line width in mm" },
                            "layer": { "type": "string", "description": "Layer name: Top Overlay, Top Assembly, Top Courtyard, Mechanical 1, etc." },
                            "solder_mask_expansion": { "type": "number", "description": "Solder mask expansion override in mm (optional; omit to use the rule default)" },
                            "keepout_restrictions": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Keepout restriction bitmask (optional; defaults to 0)" },
                            "net_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Net index into the board net list (common header, 0-65534; 65535 = no net). Normally omitted for library footprints; preserved on a read-modify-write. Default: 65535" },
                            "polygon_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                            "component_index": { "type": "integer", "minimum": -1, "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "flags": { "type": ["string", "integer"], "minimum": 0, "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"LOCKED\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom, 32=testpoint-top, 64=testpoint-bottom; 128 and above are on-disk bits read_pcblib carries verbatim as DISK_BIT_n — pass them back unchanged). Default: none" },
                            "guid": { "type": "string", "description": "Altium's stable identity for the primitive, from the footprint's PrimitiveGuids stream (braced string, e.g. \"{A5172B29-...}\"), as read_pcblib emits it. Pass it back unchanged so a read-modify-write keeps the identity Altium tracks for ECO; omit when authoring (a from-scratch primitive records none)." },
                            "raw_layer_id": { "type": "integer", "minimum": 0, "maximum": 255, "description": "The record's on-disk layer byte (0-255), emitted by read_pcblib only when it maps to no named layer (the primitive then reads as Multi-Layer). Pass it back unchanged so the rewrite keeps the byte; moving the primitive to a named layer discards it. Omit when authoring." }
                        },
                        "required": ["x1", "y1", "x2", "y2", "width", "layer"]
                    }
                },
                "vias": {
                    "type": "array",
                    "description": "Via definitions (vertical interconnects between copper layers, with a drill hole and annular ring).",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "number", "description": "X position in mm" },
                            "y": { "type": "number", "description": "Y position in mm" },
                            "diameter": { "type": "number", "description": "Annular ring outer diameter in mm" },
                            "hole_size": { "type": "number", "description": "Drill hole diameter in mm (must be smaller than diameter)" },
                            "from_layer": { "type": "string", "description": "Starting layer (default Top Layer): Top Layer, Bottom Layer, Mid-Layer 1, etc." },
                            "to_layer": { "type": "string", "description": "Ending layer (default Bottom Layer): Top Layer, Bottom Layer, Mid-Layer 1, etc." },
                            "solder_mask_expansion": { "type": "number", "description": "Solder mask expansion in mm (negative = tented). Default: 0" },
                            "solder_mask_expansion_mode": {
                                "type": "string",
                                "enum": accepted::MASK_EXPANSION_MODES,
                                "description": "Solder mask expansion mode. 'none' (the default) leaves the cached value stale so Altium takes the expansion from the design rule; 'from_rule' tells Altium the stored value is a rule result to honour as-is; 'manual' uses the stored value as hand-specified."
                            },
                            "thermal_relief_gap": { "type": "number", "description": "Thermal relief air-gap width in mm. Default: 0.254 (10 mil)" },
                            "solder_mask_expansion_from_hole_edge": { "type": "boolean", "description": "Measure solder-mask expansion from the HOLE edge instead of the pad edge. Default: false" },
                            "drill_layer_pair_type": { "type": "string", "enum": accepted::DRILL_LAYER_PAIR_TYPES, "description": "Drill-pair classification. \"through\" (default) spans the whole board; the others mark a via's place in a blind/buried drill-pair sequence." },
                            "thermal_relief_conductors": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Number of thermal relief conductors. Default: 4" },
                            "thermal_relief_width": { "type": "number", "description": "Thermal relief conductor width in mm. Default: 0.254 (10 mil)" },
                            "power_plane_connect_style": {
                                "type": "string",
                                "enum": accepted::POWER_PLANE_CONNECT_STYLES,
                                "description": "How the via connects to an internal power plane. Default: relief (thermal spokes)"
                            },
                            "power_plane_relief_expansion": { "type": "number", "description": "Power-plane relief expansion in mm. Default: 0.508 (20 mil)" },
                            "power_plane_clearance": { "type": "number", "description": "Power-plane (anti-pad) clearance in mm. Default: 0.508 (20 mil)" },
                            "paste_mask_expansion": { "type": "number", "description": "Paste-mask expansion in mm. Default: 0" },
                            "net_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Net index into the board net list (0-65534; 65535 = no net). Default: 65535" },
                            "polygon_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                            "component_index": { "type": "integer", "minimum": -1, "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                            "hole_positive_tolerance": { "type": "number", "description": "Positive drill tolerance in mm (optional; omit to leave unset)" },
                            "hole_negative_tolerance": { "type": "number", "description": "Negative drill tolerance in mm (optional; omit to leave unset)" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "flags": { "type": ["string", "integer"], "minimum": 0, "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"TENTING_TOP\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom, 32=testpoint-top, 64=testpoint-bottom; 128 and above are on-disk bits read_pcblib carries verbatim as DISK_BIT_n — pass them back unchanged). Tenting covers the via with solder mask. Default: none" },
                            "solder_mask_expansion_back": { "type": "number", "description": "Bottom-face solder mask expansion in mm (optional). Omit to use solder_mask_expansion on both faces, as Altium's own template does." },
                            "diameter_stack_mode": { "type": "string", "enum": accepted::STACK_MODES, "description": "Per-layer via diameter stack. \"simple\" (default) uses one diameter on every layer; \"top_middle_bottom\" and \"full_stack\" both take 32 per_layer_diameters entries (index 0 = Top, 1 = Bottom, 2-31 = Mid layers) — top_middle_bottom is Altium's mode in which the mid layers share one diameter." },
                            "per_layer_diameters": { "type": "array", "description": "Per-layer via diameters in mm: exactly 32 entries (index 0 = Top, 1 = Bottom, 2-31 = Mid layers), only with a diameter_stack_mode other than \"simple\"; any other count, or a stack on a simple via, is refused.", "items": { "type": "number" } },
                            "guid": { "type": "string", "description": "Altium's stable identity for the primitive, from the footprint's PrimitiveGuids stream (braced string, e.g. \"{A5172B29-...}\"), as read_pcblib emits it. Pass it back unchanged so a read-modify-write keeps the identity Altium tracks for ECO; omit when authoring (a from-scratch primitive records none)." },
                            "raw_block": { "type": "string", "description": "Base64 of the via's whole record block exactly as read_pcblib emitted it; the writer overlays the typed fields on it so unmodelled bytes (in-record GUID slots, cached values, an older library's longer block) round-trip verbatim. Pass back unchanged; omit when authoring (the template block is used)." }
                        },
                        "required": ["x", "y", "diameter", "hole_size"]
                    }
                },
                "fills": {
                    "type": "array",
                    "description": "Filled rectangle definitions (solid copper/keepout fill defined by two opposite corners).",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x1": { "type": "number", "description": "First corner X in mm" },
                            "y1": { "type": "number", "description": "First corner Y in mm" },
                            "x2": { "type": "number", "description": "Second corner X in mm" },
                            "y2": { "type": "number", "description": "Second corner Y in mm" },
                            "layer": { "type": "string", "description": "Layer name (default Top Layer): Top Layer, Bottom Layer, Top Overlay, Mechanical 1, etc." },
                            "rotation": { "type": "number", "description": "Rotation in degrees. Default: 0" },
                            "solder_mask_expansion": { "type": "number", "description": "Solder mask expansion override in mm (optional; omit to use the rule default)" },
                            "keepout_restrictions": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Keepout restriction bitmask (optional; defaults to 0)" },
                            "net_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Net index into the board net list (common header, 0-65534; 65535 = no net). Normally omitted for library footprints; preserved on a read-modify-write. Default: 65535" },
                            "polygon_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                            "component_index": { "type": "integer", "minimum": -1, "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "flags": { "type": ["string", "integer"], "minimum": 0, "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"LOCKED\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom, 32=testpoint-top, 64=testpoint-bottom; 128 and above are on-disk bits read_pcblib carries verbatim as DISK_BIT_n — pass them back unchanged). Default: none" },
                            "guid": { "type": "string", "description": "Altium's stable identity for the primitive, from the footprint's PrimitiveGuids stream (braced string, e.g. \"{A5172B29-...}\"), as read_pcblib emits it. Pass it back unchanged so a read-modify-write keeps the identity Altium tracks for ECO; omit when authoring (a from-scratch primitive records none)." },
                            "raw_layer_id": { "type": "integer", "minimum": 0, "maximum": 255, "description": "The record's on-disk layer byte (0-255), emitted by read_pcblib only when it maps to no named layer (the primitive then reads as Multi-Layer). Pass it back unchanged so the rewrite keeps the byte; moving the primitive to a named layer discards it. Omit when authoring." }
                        },
                        "required": ["x1", "y1", "x2", "y2"]
                    }
                },
                "arcs": {
                    "type": "array",
                    "description": "Arc definitions",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "number", "description": "Centre X" },
                            "y": { "type": "number", "description": "Centre Y" },
                            "radius": { "type": "number" },
                            "start_angle": { "type": "number", "description": "Start angle in degrees" },
                            "end_angle": { "type": "number", "description": "End angle in degrees" },
                            "width": { "type": "number", "description": "Line width in mm" },
                            "layer": { "type": "string", "description": "Layer name: Top Overlay, Top Assembly, Mechanical 1, etc." },
                            "solder_mask_expansion": { "type": "number", "description": "Solder mask expansion override in mm (optional; omit to use the rule default)" },
                            "keepout_restrictions": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Keepout restriction bitmask (optional; defaults to 0)" },
                            "net_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Net index into the board net list (common header, 0-65534; 65535 = no net). Normally omitted for library footprints; preserved on a read-modify-write. Default: 65535" },
                            "polygon_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                            "component_index": { "type": "integer", "minimum": -1, "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "flags": { "type": ["string", "integer"], "minimum": 0, "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"LOCKED\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom, 32=testpoint-top, 64=testpoint-bottom; 128 and above are on-disk bits read_pcblib carries verbatim as DISK_BIT_n — pass them back unchanged). Default: none" },
                            "guid": { "type": "string", "description": "Altium's stable identity for the primitive, from the footprint's PrimitiveGuids stream (braced string, e.g. \"{A5172B29-...}\"), as read_pcblib emits it. Pass it back unchanged so a read-modify-write keeps the identity Altium tracks for ECO; omit when authoring (a from-scratch primitive records none)." },
                            "raw_layer_id": { "type": "integer", "minimum": 0, "maximum": 255, "description": "The record's on-disk layer byte (0-255), emitted by read_pcblib only when it maps to no named layer (the primitive then reads as Multi-Layer). Pass it back unchanged so the rewrite keeps the byte; moving the primitive to a named layer discards it. Omit when authoring." }
                        },
                        "required": ["x", "y", "radius", "start_angle", "end_angle", "width", "layer"]
                    }
                },
                "regions": {
                    "type": "array",
                    "description": "Filled region definitions (courtyard, copper pour, cutout, etc.)",
                    "items": {
                        "type": "object",
                        "properties": {
                            "vertices": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "x": { "type": "number" },
                                        "y": { "type": "number" }
                                    }
                                }
                            },
                            "layer": { "type": "string", "description": "Layer name: Top Courtyard, Top Assembly, Mechanical 1, etc." },
                            "kind": { "type": ["string", "integer"], "minimum": 0, "examples": accepted::REGION_KINDS, "description": "Region kind (optional). \"copper\" (default) for a copper pour/fill, \"cutout\" for a board/polygon cutout, or a raw Altium KIND integer. Default: copper" },
                            "name": { "type": "string", "description": "Region name (the NAME parameter, optional). Default: empty" },
                            "net_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Net index into the board net list (optional). 65535 = no net. Default: 65535" },
                            "polygon_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                            "component_index": { "type": "integer", "minimum": -1, "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                            "cavity_height": { "type": "number", "description": "Cavity height in mm for embedded components (optional). Default: 0" },
                            "arc_resolution": { "type": "number", "description": "Altium ARCRESOLUTION (arc-to-line tolerance, optional). Normally omitted; preserved on a read-modify-write. Default: 0" },
                            "sub_poly_index": { "type": "integer", "minimum": -1, "description": "Altium SUBPOLYINDEX; -1 when not a polygon sub-shape. Preserved on a read-modify-write. Default: -1" },
                            "union_index": { "type": "integer", "minimum": 0, "description": "Altium UNIONINDEX for grouped primitives. Preserved on a read-modify-write. Default: 0" },
                            "is_shape_based": { "type": "boolean", "description": "Altium ISSHAPEBASED. Preserved on a read-modify-write. Default: false" },
                            "holes": {
                                "type": "array",
                                "description": "Interior hole/cutout contours (optional). Each hole is an array of {x,y} vertices subtracted from the outline.",
                                "items": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "x": { "type": "number" },
                                            "y": { "type": "number" }
                                        }
                                    }
                                }
                            },
                            "unique_id": { "type": "string", "description": "Unique ID (optional, 8-char alphanumeric). Default: none" },
                            "additional_parameters": { "type": "array", "description": "Unmodelled region parameter keys captured verbatim on read (e.g. board-region keys like LAYER, KEEPOUT, ISBOARDCUTOUT). Each entry is a [key, value] string pair. Round-tripped so a read-modify-write does not drop keys the tool does not model. Normally omitted; supply only the pairs read_pcblib returned.", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } },
                            "flags": { "type": ["string", "integer"], "minimum": 0, "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"LOCKED\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom, 32=testpoint-top, 64=testpoint-bottom; 128 and above are on-disk bits read_pcblib carries verbatim as DISK_BIT_n — pass them back unchanged). Default: none" },
                            "v7_layer": { "type": "string", "description": "The V7_LAYER token as read_pcblib emitted it when it disagrees with layer: a board cutout stores the keep-out layer byte with V7_LAYER naming the layer it was drawn on. Pass back unchanged; omit when authoring (derived from layer)." },
                            "param_key_order": { "type": "array", "description": "The region parameter keys in stored order, as read_pcblib emitted them; the writer replays this order so the block stays byte-faithful (a cutout stores LAYER, KEEPOUT and ISBOARDCUTOUT right after NAME). Pass back unchanged; omit when authoring (canonical order).", "items": { "type": "string" } },
                            "guid": { "type": "string", "description": "Altium's stable identity for the primitive, from the footprint's PrimitiveGuids stream (braced string, e.g. \"{A5172B29-...}\"), as read_pcblib emits it. Pass it back unchanged so a read-modify-write keeps the identity Altium tracks for ECO; omit when authoring (a from-scratch primitive records none)." }
                        },
                        "required": ["vertices", "layer"]
                    }
                },
                "text": {
                    "type": "array",
                    "description": "Text/string definitions",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "number" },
                            "y": { "type": "number" },
                            "text": { "type": "string" },
                            "height": { "type": "number", "description": "Text height in mm" },
                            "layer": { "type": "string", "description": "Layer name: Top Overlay, Top Assembly, Mechanical 1, etc." },
                            "rotation": { "type": "number", "description": "Rotation in degrees" },
                            "kind": { "type": "string", "enum": accepted::TEXT_KINDS, "description": "Text rendering kind. \"stroke\" (default) uses a vector stroke font (most common for silkscreen); \"true_type\" renders with the TrueType font named by font_name; \"bar_code\" is a barcode. Default: stroke" },
                            "stroke_font": { "type": "string", "enum": accepted::STROKE_FONTS, "description": "Stroke font selection (only meaningful when kind is \"stroke\"). Default: default (Altium's built-in stroke font)" },
                            "font_name": { "type": "string", "description": "TrueType font name, a Windows face name of at most 31 characters (only meaningful when kind is \"true_type\"). Default: Arial" },
                            "bold": { "type": "boolean", "description": "Bold font style (TrueType). Default: false" },
                            "italic": { "type": "boolean", "description": "Italic font style (TrueType). Default: false" },
                            "mirror": { "type": "boolean", "description": "Mirror the text (bottom-side silkscreen). Default: false" },
                            "is_comment": { "type": "boolean", "description": "Mark this text as the component's Comment field (Altium IsComment). Preserved on read-modify-write. Default: false" },
                            "is_designator": { "type": "boolean", "description": "Mark this text as the component's Designator field (Altium IsDesignator). Preserved on read-modify-write. Default: false" },
                            "justification": { "type": "string", "enum": accepted::TEXT_JUSTIFICATIONS, "description": "Text anchor / justification within its frame. Default: bottom_left" },
                            "stroke_width": { "type": "number", "description": "Stroke line width in mm (optional; defaults to Altium's ~4 mil)" },
                            "is_inverted": { "type": "boolean", "description": "Draw the text inverted (knockout): a filled bar with the glyphs punched out. Default: false" },
                            "inverted_border": { "type": "number", "description": "Border margin around inverted text in mm (only meaningful when is_inverted). Default: none" },
                            "use_inverted_rectangle": { "type": "boolean", "description": "Use an explicit framed rectangle (inverted_rect_width / inverted_rect_height) for the inverted text box instead of auto-sizing to the glyphs. Default: false" },
                            "inverted_rect_width": { "type": "number", "description": "Inverted-rectangle width in mm (only meaningful when use_inverted_rectangle). Default: none" },
                            "inverted_rect_height": { "type": "number", "description": "Inverted-rectangle height in mm (only meaningful when use_inverted_rectangle). Default: none" },
                            "inverted_rect_text_offset": { "type": "number", "description": "Text offset within the inverted rectangle in mm (only meaningful when use_inverted_rectangle). Default: none" },
                            "net_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Net index into the board net list (common header, 0-65534; 65535 = no net). Normally omitted for library footprints; preserved on a read-modify-write. Default: 65535" },
                            "polygon_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                            "component_index": { "type": "integer", "minimum": -1, "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "flags": { "type": ["string", "integer"], "minimum": 0, "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"LOCKED\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom, 32=testpoint-top, 64=testpoint-bottom; 128 and above are on-disk bits read_pcblib carries verbatim as DISK_BIT_n — pass them back unchanged). Default: none" },
                            "barcode_kind": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Barcode symbology (kind \"bar_code\" only): 1 = Code 128, the only one Altium names; 0 on any other text. Default: 0" },
                            "barcode_full_width": { "type": "number", "description": "Barcode overall width in mm (kind \"bar_code\" only; optional)" },
                            "barcode_full_height": { "type": "number", "description": "Barcode overall height in mm (kind \"bar_code\" only; optional)" },
                            "barcode_x_margin": { "type": "number", "description": "Barcode horizontal quiet-zone margin in mm (kind \"bar_code\" only; optional)" },
                            "barcode_y_margin": { "type": "number", "description": "Barcode vertical quiet-zone margin in mm (kind \"bar_code\" only; optional)" },
                            "barcode_font_name": { "type": "string", "description": "Font of the barcode's human-readable line, a Windows face name of at most 31 characters (kind \"bar_code\" only). Default: empty" },
                            "barcode_inverted": { "type": "boolean", "description": "Render the barcode inverted, light bars on dark (kind \"bar_code\" only). Default: false" },
                            "barcode_show_text": { "type": "boolean", "description": "Draw the human-readable line under the bars (kind \"bar_code\" only; Altium turns it on for a new barcode). Default: false" },
                            "guid": { "type": "string", "description": "Altium's stable identity for the primitive, from the footprint's PrimitiveGuids stream (braced string, e.g. \"{A5172B29-...}\"), as read_pcblib emits it. Pass it back unchanged so a read-modify-write keeps the identity Altium tracks for ECO; omit when authoring (a from-scratch primitive records none)." },
                            "raw_layer_id": { "type": "integer", "minimum": 0, "maximum": 255, "description": "The record's on-disk layer byte (0-255), emitted by read_pcblib only when it maps to no named layer (the primitive then reads as Multi-Layer). Pass it back unchanged so the rewrite keeps the byte; moving the primitive to a named layer discards it. Omit when authoring." },
                            "raw_geometry": { "type": "string", "description": "Base64 of the text's geometry block exactly as read_pcblib emitted it; the writer overlays the typed fields on it so Altium's cached render metrics round-trip verbatim. Pass back unchanged; omit when authoring (the template block is used)." }
                        },
                        "required": ["x", "y", "text", "height", "layer"]
                    }
                },
                "step_model": {
                    "type": "object",
                    "description": "Optional STEP 3D model attachment",
                    "properties": {
                        "filepath": { "type": "string", "description": "Path to .step file (for embedding) or model name (for external reference)" },
                        "embed": { "type": "boolean", "description": "If true (default), embed the STEP file. If false, create external reference only (file doesn't need to exist)" },
                        "x_offset": { "type": "number" },
                        "y_offset": { "type": "number" },
                        "z_offset": { "type": "number" },
                        "rotation": { "type": "number", "description": "Z rotation in degrees" }
                    },
                    "required": ["filepath"]
                },
                "model_3d": {
                    "type": ["object", "null"],
                    "description": "Alternative spelling of the same 3D-model reference, matching read_pcblib's output shape so a read result replays into write_pcblib unchanged. Ignored when 'step_model' is also given; null is accepted and ignored.",
                    "properties": {
                        "filepath": { "type": "string", "description": "Path to a .step file (embedded at save when it exists on disk) or a bare model name (kept as a reference)" },
                        "x_offset": { "type": "number" },
                        "y_offset": { "type": "number" },
                        "z_offset": { "type": "number" },
                        "rotation": { "type": "number", "description": "Z rotation in degrees" }
                    }
                },
                "component_bodies": {
                    "type": "array",
                    "description": "Generic extruded 3D bodies (no STEP file). Each is an extruded shape defined by an outline + heights, useful for giving parts a 3D height when no STEP model is available.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "overall_height": { "type": "number", "description": "Total body height above the board, in mm (top of extrusion)" },
                            "standoff_height": { "type": "number", "description": "Standoff from the board to the bottom of the body, in mm. Default: 0" },
                            "cavity_height": { "type": "number", "description": "Cavity depth in mm for a body embedded into a board cavity. Default: 0" },
                            "outline": {
                                "type": "array",
                                "description": "Optional 2D outline polygon as {x,y} vertices in mm. If omitted, a bounding box is auto-generated from the footprint pads.",
                                "items": {
                                    "type": "object",
                                    "properties": { "x": { "type": "number" }, "y": { "type": "number" } },
                                    "required": ["x", "y"]
                                }
                            },
                            "layer": { "type": "string", "description": "Body layer: 'Top 3D Body' (default) or 'Bottom 3D Body'" },
                            "z_offset": { "type": "number", "description": "Z offset in mm. Default: 0" },
                            "rotation_x": { "type": "number" },
                            "rotation_y": { "type": "number" },
                            "rotation_z": { "type": "number" },
                            "model_checksum": { "type": "integer", "description": "Altium MODEL.CHECKSUM; normally omitted (defaults to 0). Preserved verbatim on a read-modify-write round-trip." },
                            "name": { "type": "string", "description": "Altium NAME. Default: \" \" (a single space, as template-default bodies emit)." },
                            "kind": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Altium KIND (0=extruded, etc.). Default: 0" },
                            "sub_poly_index": { "type": "integer", "minimum": -1, "description": "Altium SUBPOLYINDEX; -1 when not a polygon sub-shape. Default: -1" },
                            "union_index": { "type": "integer", "minimum": 0, "description": "Altium UNIONINDEX for grouped primitives. Default: 0" },
                            "is_shape_based": { "type": "boolean", "description": "Altium ISSHAPEBASED (shape-based vs. model-based body). Default: false" },
                            "body_projection": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Altium BODYPROJECTION (board side). Default: 0" },
                            "body_color_3d": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "3D body colour as decimal RGB (Altium BODYCOLOR3D). Default: 8421504 (0x808080, grey)" },
                            "body_opacity_3d": { "type": "number", "description": "3D body opacity, 0.0-1.0 (Altium BODYOPACITY3D). Default: 1.0" },
                            "model_2d_rotation": { "type": "number", "description": "2D placement rotation in degrees (Altium MODEL.2D.ROTATION). Default: 0" },
                            "model_2d_x": { "type": "number", "description": "Model offset from the body origin in the 2D plane, X in mm (Altium MODEL.2D.X). Default: 0" },
                            "model_2d_y": { "type": "number", "description": "Model offset in the 2D plane, Y in mm (Altium MODEL.2D.Y). Default: 0" },
                            "model_id": { "type": "string", "description": "Model GUID referencing an embedded model (Altium MODELID). Default: \"\" (none)" },
                            "model_name": { "type": "string", "description": "Model filename or external path (Altium MODEL.NAME). Default: \"\" (none)" },
                            "embedded": { "type": "boolean", "description": "Whether the model is embedded in the library (Altium MODEL.EMBED). Default: false" },
                            "net_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Net index into the board net list (common header, 0-65534; 65535 = no net). Normally omitted for library footprints; preserved on a read-modify-write. Default: 65535" },
                            "polygon_index": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                            "component_index": { "type": "integer", "minimum": -1, "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "additional_parameters": { "type": "array", "description": "Unmodelled body parameter keys captured verbatim on read — anything the block carries that this tool has no typed field for (e.g. keys a newer Altium version writes). Each entry is a [key, value] string pair. Round-tripped so a read-modify-write does not drop keys the tool does not model. Normally omitted; supply only the pairs read_pcblib returned.", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } },
                            "identifier": { "type": "string", "description": "The body's IDENTIFIER, a user-visible name (any Unicode; stored as code points on disk). Default: empty" },
                            "texture_center_x": { "type": "string", "description": "TEXTURECENTERX verbatim as read_pcblib emitted it: Altium's UI and scripting routes disagree on the texture values, so they are carried as wire text rather than derived. Pass back unchanged; omit when authoring." },
                            "texture_center_y": { "type": "string", "description": "TEXTURECENTERY verbatim as read_pcblib emitted it: Altium's UI and scripting routes disagree on the texture values, so they are carried as wire text rather than derived. Pass back unchanged; omit when authoring." },
                            "texture_size_x": { "type": "string", "description": "TEXTURESIZEX verbatim as read_pcblib emitted it: Altium's UI and scripting routes disagree on the texture values, so they are carried as wire text rather than derived (the UI writes 0.0001mil where a script writes 0mil). Pass back unchanged; omit when authoring." },
                            "texture_size_y": { "type": "string", "description": "TEXTURESIZEY verbatim as read_pcblib emitted it: Altium's UI and scripting routes disagree on the texture values, so they are carried as wire text rather than derived. Pass back unchanged; omit when authoring." },
                            "texture_rotation": { "type": "string", "description": "TEXTUREROTATION verbatim as read_pcblib emitted it: Altium's UI and scripting routes disagree on the texture values, so they are carried as wire text rather than derived (a UI-authored body can carry a rotated texture). Pass back unchanged; omit when authoring." },
                            "v7_layer": { "type": "string", "description": "The V7_LAYER token as read_pcblib emitted it when the layer byte maps to no named layer; replayed together with raw_layer_id while the body stays on the Multi-Layer catch-all. Pass back unchanged; omit when authoring (derived from layer)." },
                            "param_key_order": { "type": "array", "description": "The body parameter keys in stored order, as read_pcblib emitted them; the writer replays this order so the block stays byte-faithful (a UI-authored body stores BODYOVERRIDECOLOR right after BODYOPACITY3D). Pass back unchanged; omit when authoring (canonical order).", "items": { "type": "string" } },
                            "guid": { "type": "string", "description": "Altium's stable identity for the primitive, from the footprint's PrimitiveGuids stream (braced string, e.g. \"{A5172B29-...}\"), as read_pcblib emits it. Pass it back unchanged so a read-modify-write keeps the identity Altium tracks for ECO; omit when authoring (a from-scratch primitive records none)." },
                            "raw_layer_id": { "type": "integer", "minimum": 0, "maximum": 255, "description": "The record's on-disk layer byte (0-255), emitted by read_pcblib only when it maps to no named layer (the primitive then reads as Multi-Layer). Pass it back unchanged so the rewrite keeps the byte; moving the primitive to a named layer discards it. Omit when authoring." }
                        },
                        "required": ["overall_height"]
                    }
                },
                "guid": { "type": "string", "description": "The footprint's own identity GUID (the PrimitiveGuids entry that names no primitive; braced string) as read_pcblib emits it. Pass it back unchanged on a read-modify-write; omit when authoring." },
                "primitive_order": { "type": "array", "description": "The footprint's primitives in Data-stream order, one kind name per primitive, as read_pcblib reports it. Passing it back keeps the source's stream order on a read-modify-write and marks the footprint as a read echo (no designator text is added). Omit when authoring: primitives are written grouped by kind.", "items": { "type": "string", "enum": crate::altium::pcblib::PrimitiveKind::WRITE_ORDER.iter().map(|k| k.name()).collect::<Vec<_>>() } }
            },
            "required": ["name"]
        })
    }

    /// The symbol object `write_schlib` takes per `symbols` entry and
    /// `update_component` takes as `symbol`; see [`Self::footprint_schema`].
    #[allow(clippy::too_many_lines)]
    fn symbol_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "storage_name": { "type": "string", "description": "The CFB storage the component was read from, as read_pcblib/read_schlib emit it. Pass it back unchanged on a read-modify-write so the component stays where Altium looks for it (Altium re-derives a short name's storage and maps a long one through SectionKeys, so a moved storage is a component it cannot load); omit when authoring or renaming, and the storage name is derived as Altium would: / \\ : ! * become _, then a cut at 31 characters." },
                "description": { "type": "string", "description": "Symbol description. Keep to 256 characters if the library will be imported into an Altium 365 workspace — that importer refuses longer ones; a longer description is written and reported as a validation warning." },
                "designator_prefix": { "type": "string", "description": "Reference-designator class letter, e.g. 'R' for resistors, 'U' for ICs. Written as '<prefix>?'. If omitted, falls back to 'component_type' (IEEE 315 / ASME Y14.44 mapping), then to 'U'." },
                "designator_x": { "type": "number", "description": "X position of the designator text. Default: -5 (Altium's from-scratch placement)" },
                "designator_y": { "type": "number", "description": "Y position of the designator text. Default: 5 (Altium's from-scratch placement)" },
                "designator_unique_id": { "type": "string", "description": "8-char unique ID of the designator record; preserved on read-modify-write, auto-generated if omitted" },
                "component_type": { "type": "string", "description": "Optional component category (e.g. 'resistor', 'capacitor', 'inductor', 'diode', 'transistor', 'connector', 'crystal', 'ic') used to derive the IEEE designator letter when 'designator_prefix' is not given. Unknown values default to 'U'." },
                "part_count": { "type": "integer", "minimum": 1, "description": "Number of parts for multi-part symbols (e.g., 2 for dual op-amp). Default: 1" },
                "display_mode_count": { "type": "integer", "minimum": 1, "description": "Number of display modes (1 = normal only, 2+ = alternate/de-Morgan views). Default: 1" },
                "current_part_id": { "type": "integer", "minimum": 1, "description": "Currently selected part (1-based). Default: 1" },
                "part_id_locked": { "type": "boolean", "description": "Whether the part selection is locked. Default: false" },
                "source_library_name": { "type": "string", "description": "Source library name recorded in the symbol header. Default: '*'" },
                "target_file_name": { "type": "string", "description": "Target file name recorded in the symbol header. Default: '*'" },
                "pins": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "designator": { "type": "string" },
                            "name": { "type": "string" },
                            "x": { "type": "integer", "description": "Pin's body-attach (INNER) end, in whole schematic units (10 units = 1 grid square). This is the end that touches the symbol body, NOT the connection tip. The pin is drawn from (x,y) extending 'length' units in the 'orientation' direction; the connection tip is at the far end. Pins are integer-positioned; for an off-grid pin supply the sub-unit remainder via 'frac' (a fractional value here is truncated)." },
                            "y": { "type": "integer", "description": "Y of the pin's body-attach (inner) end, in whole schematic units. See 'x' (use 'frac' for off-grid)." },
                            "length": { "type": "number", "description": "Pin length in schematic units (10 = 1 grid). Drawn from (x,y) outward in the 'orientation' direction." },
                            "orientation": { "type": "string", "enum": accepted::PIN_ORIENTATIONS, "description": "Direction the pin POINTS, away from the body — NOT which side it sits on. A pin on the LEFT side uses 'left' (tip at x-length); a RIGHT-side pin uses 'right' (tip at x+length); 'up'/'down' for top/bottom pins. Put each pin's (x,y) on the matching body-rectangle edge so it attaches flush, e.g. left pin {x:-50,y:20,length:30,orientation:'left'} with rectangle x1=-50, and the matching right pin {x:50,y:20,length:30,orientation:'right'} with x2=50. For TOP/BOTTOM pins, (x,y) sits on the body's top/bottom edge and the pin points outward (away from the body centre): a top-side pin uses 'up' (tip at y+length, above the body), a bottom-side pin uses 'down' (tip at y-length, below) — e.g. a vertical 2-pin part with the body near y=0: top pin {x:0,y:10,length:30,orientation:'up'} (tip at y=40), bottom pin {x:0,y:-10,length:30,orientation:'down'} (tip at y=-40)." },
                            "electrical_type": { "type": "string", "enum": accepted::PIN_ELECTRICAL_TYPES, "description": "Pin electrical type. 'tristate' is accepted as an alias for 'hi_z'. Default: passive" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number this pin belongs to (1-based). Default: 1" },
                            "hidden": { "type": "boolean", "description": "Whether the pin is hidden. Default: false" },
                            "show_name": { "type": "boolean", "description": "Whether to show the pin name. Default: true" },
                            "show_designator": { "type": "boolean", "description": "Whether to show the pin designator. Default: true" },
                            "description": { "type": "string", "description": "Pin description. Default: empty" },
                            "colour": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Pin colour (BGR integer). Default: 0" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the pin is graphically locked. Default: false" },
                            "swap_id_group": { "type": "string", "description": "Pin swap-id group, for pin-swap. Default: empty" },
                            "part_and_sequence": { "type": "string", "description": "Pin part-and-sequence swap id. Default: '|&|'" },
                            "default_value": { "type": "string", "description": "Pin default value. Default: empty" },
                            "symbol_inner_edge": { "type": "string", "enum": accepted::PIN_SYMBOLS, "description": "Decoration on the INNER edge (nearest the body), e.g. 'dot' (inversion bubble), 'clock'. Default: none" },
                            "symbol_outer_edge": { "type": "string", "enum": accepted::PIN_SYMBOLS, "description": "Decoration on the OUTER edge (furthest from the body), e.g. 'dot', 'clock'. Default: none" },
                            "symbol_inside": { "type": "string", "enum": accepted::PIN_SYMBOLS, "description": "Decoration drawn inside the pin line, e.g. 'postponed_output', 'open_collector'. Default: none" },
                            "symbol_outside": { "type": "string", "enum": accepted::PIN_SYMBOLS, "description": "Decoration drawn outside the pin line, e.g. 'right_left_signal_flow', 'analog_signal_in'. Default: none" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Pin's alternate-view (display-mode) index in the binary pin record. Default: 0" },
                            "symbol_line_width": { "type": "integer", "minimum": 0, "description": "Pin symbol line-width index. Non-zero writes a PinSymbolLineWidth auxiliary stream; 0 (default) writes none." },
                            "frac": { "type": "object", "description": "Fractional pin coordinates for off-grid pins, in 1/100000 schematic-unit steps. Non-zero writes a PinFrac auxiliary stream; omit for on-grid pins.", "properties": { "x": { "type": "integer" }, "y": { "type": "integer" }, "length": { "type": "integer", "minimum": 0 } } },
                            "is_not_accessible": { "type": "boolean", "description": "Whether the pin is marked not-accessible (the pin record's 0x20 bit). Default: false" },
                            "formal_type": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Pin formal-type byte; Altium writes 1 for a normal pin. Preserved on a read-modify-write. Default: 1" }
                        },
                        "required": ["designator", "name", "x", "y", "length", "orientation"]
                    }
                },
                "rectangles": {
                    "type": "array",
                    "description": "Rectangle definitions",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x1": { "type": "number", "description": "Left X coordinate" },
                            "y1": { "type": "number", "description": "Bottom Y coordinate" },
                            "x2": { "type": "number", "description": "Right X coordinate" },
                            "y2": { "type": "number", "description": "Top Y coordinate" },
                            "line_width": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Border width. Default: 1" },
                            "line_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Border BGR colour. Default: 0x000080" },
                            "fill_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Fill BGR colour. Default: 0xB0FFFF (Altium light yellow)" },
                            "line_style": { "type": "integer", "minimum": 0, "maximum": 2, "description": "Border line style: 0=Solid, 1=Dashed, 2=Dotted. Default: 0" },
                            "filled": { "type": "boolean", "description": "Whether filled. Default: true" },
                            "transparent": { "type": "boolean", "description": "Whether the fill is transparent. Default: false" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["x1", "y1", "x2", "y2"]
                    }
                },
                "round_rects": {
                    "type": "array",
                    "description": "Rounded-rectangle definitions",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x1": { "type": "number", "description": "Left X coordinate" },
                            "y1": { "type": "number", "description": "Bottom Y coordinate" },
                            "x2": { "type": "number", "description": "Right X coordinate" },
                            "y2": { "type": "number", "description": "Top Y coordinate" },
                            "corner_x_radius": { "type": "number", "description": "Horizontal corner radius. Default: 0" },
                            "corner_y_radius": { "type": "number", "description": "Vertical corner radius. Default: 0" },
                            "line_width": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Border width. Default: 1" },
                            "line_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Border BGR colour. Default: 0x000080" },
                            "fill_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Fill BGR colour. Default: 0xB0FFFF (Altium light yellow)" },
                            "line_style": { "type": "integer", "minimum": 0, "maximum": 2, "description": "Border line style: 0=Solid, 1=Dashed, 2=Dotted. Default: 0" },
                            "filled": { "type": "boolean", "description": "Whether filled. Default: true" },
                            "transparent": { "type": "boolean", "description": "Whether the fill is transparent. Default: false" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["x1", "y1", "x2", "y2", "corner_x_radius", "corner_y_radius"]
                    }
                },
                "lines": {
                    "type": "array",
                    "description": "Line definitions",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x1": { "type": "number", "description": "Start X coordinate" },
                            "y1": { "type": "number", "description": "Start Y coordinate" },
                            "x2": { "type": "number", "description": "End X coordinate" },
                            "y2": { "type": "number", "description": "End Y coordinate" },
                            "line_width": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Line width. Default: 1" },
                            "color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Line BGR colour. Default: 0x000080" },
                            "line_style": { "type": "integer", "minimum": 0, "maximum": 2, "description": "Line style: 0=Solid, 1=Dashed, 2=Dotted. Default: 0" },
                            "is_not_accessible": { "type": "boolean", "description": "Whether the line is marked not-accessible (Altium tags every line; default true)" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["x1", "y1", "x2", "y2"]
                    }
                },
                "polylines": {
                    "type": "array",
                    "description": "Polyline definitions (>= 2 connected points). Optional endpoint shapes turn a polyline into an arrow.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "points": {
                                "type": "array",
                                "description": "Points (>= 2) as objects with x/y in schematic units. 'vertices' is accepted as an alias.",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "x": { "type": "number" },
                                        "y": { "type": "number" }
                                    },
                                    "required": ["x", "y"]
                                }
                            },
                            "line_width": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Line width. Default: 1" },
                            "color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Line BGR colour. Default: 0x000080" },
                            "line_style": { "type": "integer", "minimum": 0, "maximum": 2, "description": "Line style: 0=Solid, 1=Dashed, 2=Dotted. Default: 0" },
                            "start_line_shape": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Start endpoint (arrowhead) shape id. Default: 0 (none)" },
                            "end_line_shape": { "type": "integer", "minimum": 0, "maximum": 255, "description": "End endpoint (arrowhead) shape id. Default: 0 (none)" },
                            "line_shape_size": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Size of the endpoint shapes. Default: 0" },
                            "transparent": { "type": "boolean", "description": "Whether the polyline is transparent. Default: false" },
                            "is_not_accessible": { "type": "boolean", "description": "Whether the polyline is marked not-accessible (Altium tags every polyline; default true)" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "vertices": { "type": "array", "description": "Alias of points, read when points is absent.", "items": { "type": "object", "properties": { "x": { "type": "number" }, "y": { "type": "number" } }, "required": ["x", "y"] } },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["points"]
                    }
                },
                "polygons": {
                    "type": "array",
                    "description": "Filled polygon definitions (>= 3 vertices)",
                    "items": {
                        "type": "object",
                        "properties": {
                            "points": {
                                "type": "array",
                                "description": "Vertices (>= 3) as objects with x/y in schematic units",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "x": { "type": "number" },
                                        "y": { "type": "number" }
                                    },
                                    "required": ["x", "y"]
                                }
                            },
                            "line_width": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Border width. Default: 1" },
                            "line_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Border BGR colour. Default: 0x000080" },
                            "fill_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Fill BGR colour. Default: 0xB0FFFF (Altium light yellow)" },
                            "line_style": { "type": "integer", "minimum": 0, "maximum": 2, "description": "Border style: 0=Solid, 1=Dashed, 2=Dotted. Default: 0" },
                            "filled": { "type": "boolean", "description": "Whether filled. Default: true" },
                            "transparent": { "type": "boolean", "description": "Whether the fill is transparent (vs opaque). Default: false" },
                            "is_not_accessible": { "type": "boolean", "description": "Whether the polygon is marked not-accessible (Altium tags every polygon; default true)" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "vertices": { "type": "array", "description": "Alias of points, read when points is absent.", "items": { "type": "object", "properties": { "x": { "type": "number" }, "y": { "type": "number" } }, "required": ["x", "y"] } },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["points"]
                    }
                },
                "arcs": {
                    "type": "array",
                    "description": "Arc/circle definitions",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "number", "description": "Centre X coordinate" },
                            "y": { "type": "number", "description": "Centre Y coordinate" },
                            "radius": { "type": "number", "description": "Radius in schematic units" },
                            "start_angle": { "type": "number", "description": "Start angle in degrees (0 = right, CCW). Default: 0" },
                            "end_angle": { "type": "number", "description": "End angle in degrees. Default: 360 (full circle)" },
                            "line_width": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Line width. Default: 1" },
                            "color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Line BGR colour. Default: 0x000080" },
                            "fill_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Fill BGR colour (maps to AreaColor). Default: 0 (no fill)" },
                            "is_not_accessible": { "type": "boolean", "description": "Whether the arc is marked not-accessible (Altium tags every arc; default true)" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["x", "y", "radius"]
                    }
                },
                "pies": {
                    "type": "array",
                    "description": "Pie (filled circular sector / wedge) definitions",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "number", "description": "Centre X coordinate" },
                            "y": { "type": "number", "description": "Centre Y coordinate" },
                            "radius": { "type": "number", "description": "Radius in schematic units" },
                            "start_angle": { "type": "number", "description": "Start angle in degrees (0 = right, CCW). Default: 0" },
                            "end_angle": { "type": "number", "description": "End angle in degrees. Default: 360" },
                            "line_width": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Border width. Default: 1" },
                            "line_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Border BGR colour. Default: 0" },
                            "fill_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Fill BGR colour (maps to AreaColor). Default: 0" },
                            "filled": { "type": "boolean", "description": "Whether the pie is filled (IsSolid). Default: true" },
                            "transparent": { "type": "boolean", "description": "Whether the fill is transparent. Default: false" },
                            "is_not_accessible": { "type": "boolean", "description": "Whether the pie is marked not-accessible (Altium tags every shape; default true)" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["x", "y", "radius"]
                    }
                },
                "images": {
                    "type": "array",
                    "description": "Embedded/linked raster image definitions (RECORD=30). The record metadata round-trips; embedded image bytes are authored via image_data (base64) and stored in the library /Storage stream.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x1": { "type": "number", "description": "First corner X (Location.X)" },
                            "y1": { "type": "number", "description": "First corner Y (Location.Y)" },
                            "x2": { "type": "number", "description": "Second corner X (Corner.X)" },
                            "y2": { "type": "number", "description": "Second corner Y (Corner.Y)" },
                            "line_width": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Border width. Default: 1" },
                            "line_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Border BGR colour. Default: 0" },
                            "line_style": { "type": "integer", "minimum": 0, "maximum": 2, "description": "Border style: 0=Solid, 1=Dashed, 2=Dotted. Default: 0" },
                            "fill_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Fill BGR colour (AreaColor). Default: 0" },
                            "filled": { "type": "boolean", "description": "Whether the box is filled (IsSolid). Default: false" },
                            "transparent": { "type": "boolean", "description": "Whether the fill is transparent. Default: false" },
                            "show_border": { "type": "boolean", "description": "Whether the border is shown. Default: false" },
                            "keep_aspect": { "type": "boolean", "description": "Whether the image keeps its aspect ratio. Default: false" },
                            "embed_image": { "type": "boolean", "description": "Whether the image bytes are embedded (vs a link to file_name). Default: false" },
                            "file_name": { "type": "string", "description": "Image file name / embedded key (Altium stores the full source file path for embedded images)" },
                            "image_data": { "type": "string", "description": "Base64-encoded raw image bytes; stored in the library /Storage stream when embed_image is true" },
                            "is_not_accessible": { "type": "boolean", "description": "Whether the image is marked not-accessible (Altium tags every shape; default true)" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["x1", "y1", "x2", "y2"]
                    }
                },
                "text_frames": {
                    "type": "array",
                    "description": "Text frame definitions (RECORD=28): a bordered multi-line text box, distinct from labels/text (frame rectangle, word-wrap, alignment, clip-to-rect).",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x1": { "type": "number", "description": "First corner X (Location.X)" },
                            "y1": { "type": "number", "description": "First corner Y (Location.Y)" },
                            "x2": { "type": "number", "description": "Second corner X (Corner.X)" },
                            "y2": { "type": "number", "description": "Second corner Y (Corner.Y)" },
                            "text": { "type": "string", "description": "Text content of the frame" },
                            "color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Border BGR colour. Default: 0" },
                            "area_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Fill BGR colour (AreaColor). Default: 16777215 (white)" },
                            "text_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Text BGR colour. Default: 0" },
                            "text_margin": { "type": "number", "description": "Margin between the frame border and the text, in schematic units. Default: 0.00005 (Altium's from-scratch default)" },
                            "line_width": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Border width. Default: 0" },
                            "line_style": { "type": "integer", "minimum": 0, "maximum": 2, "description": "Border style: 0=Solid, 1=Dashed, 2=Dotted. Default: 0" },
                            "transparent": { "type": "boolean", "description": "Whether the fill is transparent. Default: false" },
                            "font_id": { "type": "integer", "minimum": 1, "maximum": 255, "description": "Font ID (1-based index into library fonts). Default: 1" },
                            "orientation": { "type": "integer", "minimum": 0, "maximum": 3, "description": "Text orientation: 0/1/2/3 = 0/90/180/270 degrees. Default: 0" },
                            "alignment": { "type": "integer", "minimum": 0, "maximum": 2, "description": "Text alignment: 0=left, 1=centre, 2=right. Default: 1 (centre)" },
                            "is_solid": { "type": "boolean", "description": "Whether the frame is filled (IsSolid). Default: false" },
                            "show_border": { "type": "boolean", "description": "Whether the border is shown. Default: true" },
                            "word_wrap": { "type": "boolean", "description": "Whether the text word-wraps inside the frame. Default: true" },
                            "clip_to_rect": { "type": "boolean", "description": "Whether the text is clipped to the frame rectangle. Default: true" },
                            "is_not_accessible": { "type": "boolean", "description": "Whether the frame is marked not-accessible (Altium tags every shape; default true)" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["x1", "y1", "x2", "y2", "text"]
                    }
                },
                "beziers": {
                    "type": "array",
                    "description": "Cubic Bezier curve definitions (four control points)",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x1": { "type": "number", "description": "First control point X" },
                            "y1": { "type": "number", "description": "First control point Y" },
                            "x2": { "type": "number", "description": "Second control point X" },
                            "y2": { "type": "number", "description": "Second control point Y" },
                            "x3": { "type": "number", "description": "Third control point X" },
                            "y3": { "type": "number", "description": "Third control point Y" },
                            "x4": { "type": "number", "description": "Fourth control point X" },
                            "y4": { "type": "number", "description": "Fourth control point Y" },
                            "line_width": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Curve width. Default: 1" },
                            "color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Curve BGR colour. Default: 0x000080 (128, dark red)" },
                            "is_not_accessible": { "type": "boolean", "description": "Whether the curve is marked not-accessible (Altium tags every shape; default true)" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["x1", "y1", "x2", "y2", "x3", "y3", "x4", "y4"]
                    }
                },
                "elliptical_arcs": {
                    "type": "array",
                    "description": "Elliptical arc definitions (arc with independent X/Y radii)",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "number", "description": "Centre X coordinate" },
                            "y": { "type": "number", "description": "Centre Y coordinate" },
                            "radius": { "type": "number", "description": "Primary (X) radius in schematic units" },
                            "secondary_radius": { "type": "number", "description": "Secondary (Y) radius in schematic units" },
                            "start_angle": { "type": "number", "description": "Start angle in degrees (0 = right, CCW). Default: 0" },
                            "end_angle": { "type": "number", "description": "End angle in degrees. Default: 360" },
                            "line_width": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Arc width. Default: 1" },
                            "color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Arc BGR colour. Default: 0x000080 (128, dark red)" },
                            "fill_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Fill BGR colour (AreaColor). Default: 0" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["x", "y", "radius", "secondary_radius"]
                    }
                },
                "ellipses": {
                    "type": "array",
                    "description": "Ellipse definitions",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "number", "description": "Centre X coordinate" },
                            "y": { "type": "number", "description": "Centre Y coordinate" },
                            "radius_x": { "type": "number", "description": "Horizontal radius" },
                            "radius_y": { "type": "number", "description": "Vertical radius" },
                            "line_width": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Border width. Default: 1" },
                            "line_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Border BGR colour. Default: 0x000080" },
                            "fill_color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "Fill BGR colour. Default: 0xB0FFFF (Altium light yellow)" },
                            "filled": { "type": "boolean", "description": "Whether filled. Default: true" },
                            "transparent": { "type": "boolean", "description": "Whether the fill is transparent. Default: false" },
                            "is_not_accessible": { "type": "boolean", "description": "Whether the ellipse is marked not-accessible (Altium tags every ellipse; default true)" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["x", "y", "radius_x", "radius_y"]
                    }
                },
                "labels": {
                    "type": "array",
                    "description": "Text string definitions (RECORD=4) — Altium's only free text on a symbol.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "number", "description": "X position" },
                            "y": { "type": "number", "description": "Y position" },
                            "text": { "type": "string", "description": "Text content" },
                            "font_id": { "type": "integer", "minimum": 1, "maximum": 255, "description": "Font ID. Default: 1" },
                            "color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "BGR colour. Default: 0x000080" },
                            "justification": { "type": "string", "enum": accepted::TEXT_JUSTIFICATIONS, "description": "Alignment. Default: bottom_left" },
                            "rotation": { "type": "number", "description": "Rotation in degrees. Default: 0" },
                            "is_mirrored": { "type": "boolean", "description": "Mirrored. Default: false" },
                            "is_hidden": { "type": "boolean", "description": "Hidden. Default: false" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                            "hidden": { "type": "boolean", "description": "Alias of is_hidden. Default: false" },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["x", "y", "text"]
                    }
                },
                "ieee_symbols": {
                    "type": "array",
                    "description": "IEEE symbol glyphs (RECORD=3): a dot, a clock, an active-low input, ... placed at a point with a scale.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "x": { "type": "number", "description": "Anchor X" },
                            "y": { "type": "number", "description": "Anchor Y" },
                            "symbol": { "type": "integer", "minimum": 1, "maximum": 34, "description": "Glyph, as Altium's TIeeeSymbol id: 1 Dot, 2 Right-Left Signal Flow, 3 Clock, 4 Active Low Input, 5 Analog Signal In, 6 Not Logic Connection, 7 Shift Right, 8 Postponed Output, 9 Open Collector, 10 Hi-Z, 11 High Current, 12 Pulse, 13 Schmitt, 14 Delay, 15 Group Line, 16 Group Binary, 17 Active Low Output, 18 Pi, 19 Greater Equal, 20 Less Equal, 21 Sigma, 22 Open Collector Pull Up, 23 Open Emitter, 24 Open Emitter Pull Up, 25 Digital Signal In, 26 And, 27 Invertor, 28 Or, 29 Xor, 30 Shift Left, 31 Input Output, 32 Open Circuit Output, 33 Left-Right Signal Flow, 34 Bidirectional Signal Flow" },
                            "scale_factor": { "type": "number", "description": "Glyph size in schematic units. Default: 10" },
                            "rotation": { "type": "number", "description": "Rotation in degrees (0/90/180/270). Default: 0" },
                            "is_mirrored": { "type": "boolean", "description": "Mirrored. Default: false" },
                            "line_width": { "type": "integer", "minimum": 0, "maximum": 3, "description": "Line width (0=smallest, 1=small, 2=medium, 3=large). Default: 1" },
                            "color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "BGR colour. Default: 0" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["x", "y", "symbol"]
                    }
                },
                "parameters": {
                    "type": "array",
                    "description": "Symbol parameters (e.g., Value, Part Number)",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Parameter name (e.g., 'Value')" },
                            "value": { "type": "string", "description": "Parameter value (e.g., '10k'). Default: '*'" },
                            "x": { "type": "number", "description": "X position. Default: 0" },
                            "y": { "type": "number", "description": "Y position. Default: 0" },
                            "font_id": { "type": "integer", "minimum": 1, "maximum": 255, "description": "Font ID. Default: 1" },
                            "color": { "type": "integer", "minimum": 0, "maximum": 16_777_215, "description": "BGR colour. Default: 0x800000 (dark blue)" },
                            "hidden": { "type": "boolean", "description": "Whether hidden. Default: false" },
                            "read_only_state": { "type": "integer", "minimum": 0, "maximum": 1, "description": "Read-only state (0=editable, 1=read-only). Default: 0" },
                            "param_type": { "type": "integer", "minimum": 0, "maximum": 3, "description": "Parameter type (0=String, 1=Boolean, 2=Integer, 3=Float). Default: 0" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID. Default: auto-generated" },
                            "orientation": { "type": "integer", "minimum": 0, "maximum": 3, "description": "Text orientation (0/1/2/3 = 0/90/180/270 degrees). Default: 0" },
                            "justification": { "type": "integer", "minimum": 0, "maximum": 8, "description": "Text anchor id 0-8 (0=bottom-left, 4=middle-centre, 8=top-right). Default: 0" },
                            "show_name": { "type": "boolean", "description": "Whether the parameter name is shown alongside the value. Default: false" },
                            "hide_name": { "type": "boolean", "description": "Whether the parameter name is hidden (only the value shown). Default: false" },
                            "is_mirrored": { "type": "boolean", "description": "Whether the parameter text is mirrored. Default: false" },
                            "description": { "type": "string", "description": "Parameter description text. Default: empty" },
                            "is_configurable": { "type": "boolean", "description": "Whether the parameter is variant-configurable. Default: false" },
                            "auto_position": { "type": "boolean", "description": "Whether Altium auto-positions the parameter label relative to the component. Stored inverted on the wire (NotAutoPosition=T) and only when turned off. Default: true" },
                            "is_rule": { "type": "boolean", "description": "Whether the parameter carries a PCB design-rule directive. Default: false" },
                            "is_system_parameter": { "type": "boolean", "description": "Whether this is a system parameter rather than a user one. Default: false" },
                            "text_horz_anchor": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Horizontal text-box anchor, distinct from justification. Default: 0" },
                            "text_vert_anchor": { "type": "integer", "minimum": 0, "maximum": 255, "description": "Vertical text-box anchor. Default: 0" },
                            "owner_part_id": { "type": "integer", "minimum": -1, "description": "Part number (1-based). Default: 1" },
                            "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                            "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                            "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                            "owner_part_display_mode": { "type": "integer", "minimum": 0, "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                            "raw_params": { "type": "array", "description": "The record's segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["name"]
                    }
                },
                "footprints": {
                    "type": "array",
                    "description": "Footprint model references (links to PCB footprints)",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Footprint name (entity in the PcbLib)" },
                            "description": { "type": "string", "description": "Model description" },
                            "library_path": { "type": "string", "description": "Optional absolute path to the .PcbLib containing the footprint, written as ModelDatafile0 so Altium resolves/previews the model. Omit to link by name only (requires the library to be installed/in the project)." },
                            "is_current": { "type": "boolean", "description": "Whether this is the current (default) footprint model (IsCurrent=T). Read-preserved; on write the first model is emitted as current. Default: false" },
                            "unique_id": { "type": "string", "description": "8-char Altium unique ID of the model link; preserved on read-modify-write, auto-generated if omitted" },
                            "raw_params": { "type": "array", "description": "The model link's (RECORD=45) segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays them verbatim unless the field behind one was edited, so the record comes back as Altium wrote it (the UI omits LineWidth=1, a script does not). Pass back unchanged; omit when authoring (the canonical form is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["name"]
                    }
                },
                "designator": { "type": "string", "description": "The full reference-designator text, e.g. 'R?' or 'U1', as read_schlib emits it; takes precedence over designator_prefix and component_type. Default: '<prefix>?'" },
                "all_pin_count": { "type": "integer", "minimum": 0, "description": "AllPinCount as stored in the header. Altium keeps a stale value here, so read_schlib emits it and a read-modify-write passes it back unchanged; omit when authoring (the pin count is written)." },
                "header_params": { "type": "array", "description": "The header record (RECORD=1) segments as [key, value] string pairs in stored order, exactly as read_schlib emitted them; the writer replays each verbatim unless the field behind it was edited, so the header comes back byte for byte. Pass back unchanged; omit when authoring (the canonical header is written).", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } },
                "extra_streams": { "type": "array", "description": "Streams of the symbol's storage this tool does not model (a PinFunctionData from a newer Altium, say) as [name, base64] pairs, exactly as read_schlib emitted them; written back beside the modelled ones so nothing Altium stored is dropped. Pass back unchanged; omit when authoring.", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } },
                "primitive_order": { "type": "array", "description": "The symbol's content records in stored order, one kind name per record, as read_schlib reports it; Altium numbers records in this sequence, so passing it back keeps IndexInSheet stable on a read-modify-write. Omit when authoring: records are written in the tool's own order, body graphics first.", "items": { "type": "string", "enum": crate::altium::schlib::SchPrimitiveKind::WRITE_ORDER.iter().map(|k| k.name()).collect::<Vec<_>>() } }
            },
            "required": ["name"]
        })
    }

    /// `schema` captioned for the tool it appears in.
    fn described(mut schema: serde_json::Value, description: &str) -> serde_json::Value {
        schema["description"] = json!(description);
        schema
    }

    /// Tool schemas for the library-writing family.
    #[allow(clippy::too_many_lines)]
    fn writing_tool_definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "write_pcblib".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "write_pcblib", "arguments": {"filepath": "./Passives.PcbLib", "footprints": [{"name": "RESC1608X55N", "description": "Chip resistor, 0603 (1608 metric)", "pads": [{"designator": "1", "x": -0.75, "y": 0, "width": 0.9, "height": 0.95}, {"designator": "2", "x": 0.75, "y": 0, "width": 0.9, "height": 0.95}], "tracks": [{"x1": -0.8, "y1": -0.425, "x2": 0.8, "y2": -0.425, "width": 0.12, "layer": "Top Overlay"}, {"x1": -0.8, "y1": 0.425, "x2": 0.8, "y2": 0.425, "width": 0.12, "layer": "Top Overlay"}], "regions": [{"vertices": [{"x": -1.45, "y": -0.73}, {"x": 1.45, "y": -0.73}, {"x": 1.45, "y": 0.73}, {"x": -1.45, "y": 0.73}], "layer": "Top Courtyard"}]}], "append": false}})),
                description: Some(
                    "Write footprints to an Altium .PcbLib file (set 'append': true to add to an \
                     existing library instead of replacing it). Each footprint is defined by \
                     its primitives: pads (with position, size, shape, layer), tracks, vias, \
                     fills, arcs, regions, text and component_bodies. The AI is responsible for \
                     calculating correct positions and sizes based on IPC-7351B or other standards. \
                     All coordinates and dimensions must be in millimetres (mm). A footprint \
                     authored without a '.Designator' text receives one on the Top Overlay \
                     automatically, just above its topmost pad, so every placed part shows its \
                     reference designator: supply your own to control its placement, or set \
                     'auto_designator': false to omit it; a footprint echoed back from a read \
                     (carrying primitive_order) is never touched. \
                     The response 'bodies' array echoes each footprint's 3D body height and source; \
                     a footprint with no STEP model and no component body reports source 'none'. \
                     Set 'auto_3d_body': true to have an extruded placeholder body (default height \
                     1.0 mm, flagged 'assumed_height': true) added to such footprints, then confirm \
                     or override it by supplying 'component_bodies' explicitly. The response also includes a \
                     'warnings' array flagging silkscreen (overlay) tracks that overlap a pad \
                     (silk-on-pad) so you can move them clear. No text field may contain '|', the \
                     separator of Altium's record format, which cannot hold it."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib file to create/modify"
                        },
                        "footprints": {
                            "type": "array",
                            "description": "Array of footprint definitions",
                            "items": Self::footprint_schema()
                        },
                        "append": {
                            "type": "boolean",
                            "description": "If true, append to existing file; if false, create new file"
                        },
                        "auto_3d_body": {
                            "type": "boolean",
                            "description": "If true, footprints with pads but no STEP model and no component body get a placeholder extruded 3D body (1.0 mm tall, flagged assumed_height). Default false: nothing is added unless you ask, since many footprints (fiducials, test points, mounting holes) legitimately have no body. Prefer supplying real heights via component_bodies."
                        },
                        "auto_designator": {
                            "type": "boolean",
                            "description": "If true (default), a footprint authored without a '.Designator' text gets one on the Top Overlay just above its topmost pad, so the placed part shows its reference designator. Never applied to a footprint echoed back from read_pcblib/get_component (one carrying primitive_order): Altium's own library footprints carry no designator text, and a read-modify-write must not add primitives. Set false to author a footprint without one."
                        }
                    },
                    "required": ["filepath", "footprints"]
                }),
            },
            ToolDefinition {
                name: "write_schlib".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({
                    "name": "write_schlib",
                    "arguments": {
                        "filepath": "./MyLibrary.SchLib",
                        "symbols": [{
                            "name": "R",
                            "designator_prefix": "R",
                            "pins": [
                                {"designator": "1", "name": "1", "x": -50, "y": 0, "length": 20, "orientation": "left", "electrical_type": "passive"},
                                {"designator": "2", "name": "2", "x": 50, "y": 0, "length": 20, "orientation": "right", "electrical_type": "passive"}
                            ],
                            "rectangles": [{"x1": -50, "y1": -20, "x2": 50, "y2": 20}],
                            "parameters": [{"name": "Value", "value": "10k"}],
                            "footprints": [{"name": "R0402", "library_path": "./MyLibrary.PcbLib"}]
                        }]
                    }
                })),
                description: Some(
                    "Write schematic symbols to an Altium .SchLib file (set 'append': true to add \
                     to an existing library instead of replacing it). Each symbol is defined by \
                     its primitives: pins, rectangles, round_rects, lines, polylines, polygons, \
                     arcs, pies, images, text_frames, beziers, ellipses, elliptical_arcs, labels, \
                     and text — plus its designator, parameters (Value, Manufacturer, ...) and \
                     footprint links ('footprints', name + optional library_path). Multi-part \
                     symbols set 'part_count' and tag each pin with 'owner_part_id'. \
                     Coordinates must be in schematic units (10 units = 1 grid square, not mm); \
                     a pin's (x, y) is its body-attach end and 'orientation' is the direction it \
                     points outward — the response echoes each pin's computed body_end and tip. \
                     No text field may contain '|', the separator of Altium's record format; \
                     Altium's own editor stores it as '¦' (U+00A6)."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .SchLib file to create/modify"
                        },
                        "symbols": {
                            "type": "array",
                            "description": "Array of symbol definitions",
                            "items": Self::symbol_schema()
                        },
                        "append": {
                            "type": "boolean",
                            "description": "If true, append to existing file; if false, create new file"
                        }
                    },
                    "required": ["filepath", "symbols"]
                }),
            },
            ToolDefinition {
                name: "write_libpkg".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({
                    "name": "write_libpkg",
                    "arguments": {
                        "filepath": "./MyLibrary.LibPkg",
                        "documents": ["MyLibrary.SchLib", "MyLibrary.PcbLib"]
                    }
                })),
                description: Some(
                    "Write an Altium Library Package (.LibPkg) project file that groups source \
                     library documents (.SchLib and .PcbLib) so they can be compiled into an \
                     Integrated Library (.IntLib). Member documents are referenced by their path \
                     relative to the .LibPkg. This generates only the project source; compiling \
                     to a binary .IntLib is a one-click operation inside Altium Designer \
                     (Project > Compile Integrated Library)."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .LibPkg file to create"
                        },
                        "documents": {
                            "type": "array",
                            "description": "Member document paths (.SchLib / .PcbLib). Each is referenced relative to the .LibPkg location; same-folder files become bare names.",
                            "items": { "type": "string" }
                        }
                    },
                    "required": ["filepath", "documents"]
                }),
            },
        ]
    }

    /// Tool schemas for the library-management family.
    #[allow(clippy::too_many_lines)]
    fn management_tool_definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "delete_component".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "delete_component", "arguments": {"filepath": "./MyLibrary.PcbLib", "component_names": ["OLD_FOOTPRINT", "UNUSED_COMPONENT"], "dry_run": false}})),
                description: Some(
                    "Delete one or more components from an Altium library file (.PcbLib or .SchLib). \
                     The file type is auto-detected from the extension. Returns status for each \
                     component: deleted, not_found, or error. Use dry_run=true to preview changes \
                     without modifying the file."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib or .SchLib file"
                        },
                        "component_names": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Names of components to delete"
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "If true, show what would be deleted without actually modifying the file",
                            "default": false
                        }
                    },
                    "required": ["filepath", "component_names"]
                }),
            },
            ToolDefinition {
                name: "validate_library".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "validate_library", "arguments": {"filepath": "./MyLibrary.PcbLib"}})),
                description: Some(
                    "Validate an Altium library file for common issues. Checks for: empty components \
                     (no pads/pins), duplicate designators, invalid coordinates, zero-size primitives, \
                     overlapping pads, 3D bodies whose embedded model the library does not contain, \
                     embedded models no footprint references, and other integrity problems. Returns a \
                     list of warnings and errors."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib or .SchLib file"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            ToolDefinition {
                name: "export_library".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "export_library", "arguments": {"filepath": "./MyLibrary.PcbLib", "format": "json", "compact": true}})),
                description: Some(
                    "Export a .PcbLib or .SchLib as text in the response — nothing is \
                     written to disk. JSON carries every component in the same shape \
                     read_pcblib/read_schlib and write_pcblib/write_schlib use, plus for a \
                     PcbLib the embedded 3D models its bodies reference \
                     (`embedded_models`, base64 STEP data keyed by model GUID), so \
                     import_library can rebuild the library elsewhere; CSV is a summary \
                     table: name, description (and a symbol's designator), one count \
                     column per primitive kind, and the external 3D model / footprint link \
                     count. Use JSON to version-control, back up or move a library and CSV \
                     for an inventory; use read_pcblib/read_schlib instead to inspect \
                     components with pagination, and list_components with details for a \
                     quick overview."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib or .SchLib file"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["json", "csv"],
                            "description": "Export format: 'json' for full data, 'csv' for summary table"
                        },
                        "compact": {
                            "type": "boolean",
                            "description": "For PcbLib JSON export: if true (default), omit per-layer pad data when stack_mode is Simple"
                        }
                    },
                    "required": ["filepath", "format"]
                }),
            },
            ToolDefinition {
                name: "import_library".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({
                    "name": "import_library",
                    "arguments": {
                        "output_path": "./MyLibrary.PcbLib",
                        "json_data": {
                            "file_type": "PcbLib",
                            "footprints": [{"name": "R0402", "pads": []}]
                        }
                    }
                })),
                description: Some(
                    "Import components from JSON data into an Altium library file. Accepts JSON \
                     in the format produced by export_library, enabling round-trip workflows: a \
                     PcbLib export's `embedded_models` are restored alongside the footprints, and \
                     a body whose model the data does not contain is reported in warnings. \
                     Auto-detects library type (PcbLib/SchLib) from the JSON data."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "output_path": {
                            "type": "string",
                            "description": "Path where the new library file will be created (.PcbLib or .SchLib)"
                        },
                        "json_data": {
                            "type": "object",
                            "description": "JSON data containing components to import. Should have 'file_type' (PcbLib/SchLib) and 'footprints' or 'symbols' array."
                        },
                        "append": {
                            "type": "boolean",
                            "description": "If true, append to existing library instead of overwriting. Default: false"
                        }
                    },
                    "required": ["output_path", "json_data"]
                }),
            },
            ToolDefinition {
                name: "extract_step_model".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "extract_step_model", "arguments": {"filepath": "./MyLibrary.PcbLib", "output_path": "./extracted_model.step", "model": "RESC1005X04L.step", "mode": "auto"}})),
                description: Some(
                    "Extract embedded STEP 3D models from an Altium .PcbLib file. \
                     Models are stored compressed inside the library and this tool extracts \
                     them to standalone .step files. Supports multiple modes: 'auto' (default), \
                     'list', 'extract_all', or 'extract_by_footprint'."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib file containing embedded 3D models"
                        },
                        "mode": {
                            "type": "string",
                            "enum": ["auto", "list", "extract_all", "extract_by_footprint"],
                            "description": "Extraction mode: 'auto' (default) extracts single model or lists if multiple; 'list' always lists models; 'extract_all' extracts all models to output_dir; 'extract_by_footprint' extracts models used by specified footprint"
                        },
                        "output_path": {
                            "type": "string",
                            "description": "Meaning depends only on the mode, never on how many models match: for 'auto' it is the FILE path for the extracted .step; for 'extract_all' and 'extract_by_footprint' it is a DIRECTORY that receives one file per model (created if absent). Omit to get the model inline as base64 ('auto' single model, or 'extract_by_footprint' with a single match)."
                        },
                        "model": {
                            "type": "string",
                            "description": "Model name (e.g., 'RESC1005X04L.step') or GUID to extract (for 'auto' mode)"
                        },
                        "footprint_name": {
                            "type": "string",
                            "description": "Footprint name to extract models for (required for 'extract_by_footprint' mode)"
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Maximum number of models to list, 1 or more (for 'list' mode)"
                        },
                        "offset": {
                            "type": "integer",
                            "minimum": 0,
                            "description": "Number of models to skip when listing, 0 or more (for 'list' mode)"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            ToolDefinition {
                name: "diff_libraries".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "diff_libraries", "arguments": {"filepath_a": "./OldLibrary.PcbLib", "filepath_b": "./NewLibrary.PcbLib"}})),
                description: Some(
                    "Compare two Altium library files and report differences. Shows added, removed, \
                     and modified components. Both files must be the same type (.PcbLib or .SchLib)."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath_a": {
                            "type": "string",
                            "description": "Path to the first (base/old) library file"
                        },
                        "filepath_b": {
                            "type": "string",
                            "description": "Path to the second (new/changed) library file"
                        }
                    },
                    "required": ["filepath_a", "filepath_b"]
                }),
            },
            ToolDefinition {
                name: "batch_update".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "batch_update", "arguments": {"filepath": "./MyLibrary.PcbLib", "operation": "update_track_width", "parameters": {"from_width": 0.2, "to_width": 0.25, "tolerance": 0.001}}})),
                description: Some(
                    "Perform one batch operation across all components in an Altium library file. \
                     PcbLib: 'update_track_width' (change every track of from_width to to_width, \
                     within tolerance) and 'rename_layer' (move every primitive from from_layer to \
                     to_layer). SchLib: 'update_parameters' (set parameter values across symbols). \
                     Use dry_run=true to preview changes without modifying the file."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium library file (.PcbLib or .SchLib)"
                        },
                        "operation": {
                            "type": "string",
                            "enum": ["update_track_width", "rename_layer", "update_parameters"],
                            "description": "The batch operation to perform. PcbLib: update_track_width, rename_layer. SchLib: update_parameters."
                        },
                        "parameters": {
                            "type": "object",
                            "description": "Operation-specific parameters",
                            "properties": {
                                "from_width": {
                                    "type": "number",
                                    "description": "For update_track_width: the track width to match (in mm)"
                                },
                                "to_width": {
                                    "type": "number",
                                    "description": "For update_track_width: the new track width (in mm)"
                                },
                                "from_layer": {
                                    "type": "string",
                                    "description": "For rename_layer: source layer (e.g., Mechanical 1, Top Assembly)"
                                },
                                "to_layer": {
                                    "type": "string",
                                    "description": "For rename_layer: target layer (e.g., Mechanical 2, Top Courtyard)"
                                },
                                "tolerance": {
                                    "type": "number",
                                    "description": "For update_track_width: matching tolerance (default: 0.001 mm)"
                                },
                                "param_name": {
                                    "type": "string",
                                    "description": "For update_parameters: parameter name to update (e.g., 'Value'); matched without regard to case, as Altium treats parameter names"
                                },
                                "param_value": {
                                    "type": "string",
                                    "description": "For update_parameters: new value for the parameter"
                                },
                                "symbol_filter": {
                                    "type": "string",
                                    "description": "For update_parameters: regex pattern to filter symbol names (optional)"
                                },
                                "add_if_missing": {
                                    "type": "boolean",
                                    "description": "For update_parameters: add parameter if not present (default: false)"
                                }
                            }
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "If true, show what would be updated without actually modifying the file",
                            "default": false
                        }
                    },
                    "required": ["filepath", "operation", "parameters"]
                }),
            },
            ToolDefinition {
                name: "copy_component".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "copy_component", "arguments": {"filepath": "./MyLibrary.PcbLib", "source_name": "RESC0603_IPC_MEDIUM", "target_name": "RESC0603_IPC_MEDIUM_V2", "description": "0603 resistor variant 2"}})),
                description: Some(
                    "Copy/duplicate a component within an Altium library file. Creates a new component \
                     with a different name and identical primitives, but its own identity: the copy's \
                     GUIDs and unique ids are minted fresh rather than shared with the original. \
                     Useful for creating variants."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium library file (.PcbLib or .SchLib)"
                        },
                        "source_name": {
                            "type": "string",
                            "description": "Name of the component to copy"
                        },
                        "target_name": {
                            "type": "string",
                            "description": "Name for the new copied component"
                        },
                        "description": {
                            "type": "string",
                            "description": "Optional description for the new component (defaults to source description)"
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "If true, validate the operation without modifying the file. Default: false"
                        }
                    },
                    "required": ["filepath", "source_name", "target_name"]
                }),
            },
            ToolDefinition {
                name: "rename_component".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "rename_component", "arguments": {"filepath": "./MyLibrary.PcbLib", "old_name": "RESC0603_OLD", "new_name": "RESC0603_NEW"}})),
                description: Some(
                    "Rename a component within an Altium library file. This is an atomic operation \
                     that changes the component's name while preserving all primitives and properties. \
                     More efficient than copy + delete for simple renames."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium library file (.PcbLib or .SchLib)"
                        },
                        "old_name": {
                            "type": "string",
                            "description": "Current name of the component to rename"
                        },
                        "new_name": {
                            "type": "string",
                            "description": "New name for the component"
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "If true, validate the operation without modifying the file. Default: false"
                        }
                    },
                    "required": ["filepath", "old_name", "new_name"]
                }),
            },
            ToolDefinition {
                name: "copy_component_cross_library".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "copy_component_cross_library", "arguments": {"source_filepath": "./SourceLibrary.PcbLib", "target_filepath": "./TargetLibrary.PcbLib", "component_name": "RESC0603_IPC_MEDIUM", "new_name": "RESC0603_COPIED", "description": "Copied from SourceLibrary", "ignore_missing_models": false, "preserve_external_paths": false}})),
                description: Some(
                    "Copy a component from one Altium library to another. Both libraries must be \
                     the same type (PcbLib to PcbLib, or SchLib to SchLib), and different files \
                     (use copy_component to duplicate within a library). The component keeps its \
                     identity, and the embedded 3D models its bodies reference travel with it; an \
                     external STEP file reference is dropped with a warning unless \
                     preserve_external_paths is true, since a path relative to the source library \
                     rarely resolves elsewhere. Useful for consolidating libraries or sharing \
                     components between projects."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "source_filepath": {
                            "type": "string",
                            "description": "Path to the source library file (.PcbLib or .SchLib)"
                        },
                        "target_filepath": {
                            "type": "string",
                            "description": "Path to the target library file (must be same type as source)"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Name of the component to copy from the source library"
                        },
                        "new_name": {
                            "type": "string",
                            "description": "Optional new name for the component in the target library (defaults to original name)"
                        },
                        "description": {
                            "type": "string",
                            "description": "Optional new description for the component (defaults to original description)"
                        },
                        "ignore_missing_models": {
                            "type": "boolean",
                            "description": "If true, copy the component even if referenced embedded 3D models are missing (PcbLib only). The component body references will be removed. Defaults to false."
                        },
                        "preserve_external_paths": {
                            "type": "boolean",
                            "description": "If true, preserve external 3D model paths (model_3d field) instead of removing them. The path may need manual adjustment in the target location. Defaults to false."
                        }
                    },
                    "required": ["source_filepath", "target_filepath", "component_name"]
                }),
            },
            ToolDefinition {
                name: "merge_libraries".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "merge_libraries", "arguments": {"source_filepaths": ["./LibraryA.PcbLib", "./LibraryB.PcbLib", "./LibraryC.PcbLib"], "target_filepath": "./MergedLibrary.PcbLib", "on_duplicate": "skip"}})),
                description: Some(
                    "Merge multiple Altium libraries into a single library. All source libraries must \
                     be the same type (all PcbLib or all SchLib). Components are copied from each \
                     source into the target library, together with the embedded 3D models their \
                     bodies reference (a model shared by several footprints is copied once; a body \
                     whose model is missing from its source is merged as-is and reported in \
                     warnings). External STEP file references are carried unchanged. Use dry_run=true \
                     to preview what would be merged."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "source_filepaths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Array of paths to source library files (.PcbLib or .SchLib)"
                        },
                        "target_filepath": {
                            "type": "string",
                            "description": "Path to the target library file (will be created or appended to)"
                        },
                        "on_duplicate": {
                            "type": "string",
                            "enum": ["skip", "error", "rename"],
                            "description": "How to handle duplicate component names: 'skip' (ignore duplicates), 'error' (fail on duplicates), 'rename' (auto-rename with suffix). Default: 'error'"
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "If true, show what would be merged without actually modifying any files",
                            "default": false
                        }
                    },
                    "required": ["source_filepaths", "target_filepath"]
                }),
            },
            ToolDefinition {
                name: "reorder_components".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "reorder_components", "arguments": {"filepath": "./MyLibrary.PcbLib", "component_order": ["RESC1608X55N", "RESC0805X40N", "RESC0402X20N"]}})),
                description: Some(
                    "Reorder components in an Altium library file (.PcbLib or .SchLib). Specify the \
                     desired order as a list of component names. Components not in the list are placed \
                     at the end in their original relative order."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib or .SchLib file"
                        },
                        "component_order": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Component names in desired order"
                        }
                    },
                    "required": ["filepath", "component_order"]
                }),
            },
            ToolDefinition {
                name: "update_component".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "update_component", "arguments": {"filepath": "./MyLibrary.PcbLib", "component_name": "RESC0402X20N", "footprint": {"name": "RESC0402X20N", "description": "Updated resistor 0402", "pads": [{"designator": "1", "x": -0.5, "y": 0, "width": 0.5, "height": 0.5, "layer": "TopLayer"}, {"designator": "2", "x": 0.5, "y": 0, "width": 0.5, "height": 0.5, "layer": "TopLayer"}]}}})),
                description: Some(
                    "Update a component in-place within an Altium library file, preserving its position. \
                     For PcbLib, provide a footprint object. For SchLib, provide a symbol object. The \
                     component is matched by name. Use dry_run=true to preview changes without modifying."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib or .SchLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Name of the component to update (must exist in library)"
                        },
                        "footprint": Self::described(
                            Self::footprint_schema(),
                            "For PcbLib: the footprint to store, in the shape write_pcblib takes per footprints entry"
                        ),
                        "symbol": Self::described(
                            Self::symbol_schema(),
                            "For SchLib: the symbol to store, in the shape write_schlib takes per symbols entry"
                        ),
                        "dry_run": {
                            "type": "boolean",
                            "description": "If true, show what would be updated without actually modifying the file",
                            "default": false
                        }
                    },
                    "required": ["filepath", "component_name"]
                }),
            },
            ToolDefinition {
                name: "search_components".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "search_components", "arguments": {"filepaths": ["./Resistors.PcbLib", "./Capacitors.PcbLib", "./ICs.PcbLib"], "pattern": "SOIC-*", "pattern_type": "glob"}})),
                description: Some(
                    "Search for components across multiple Altium libraries using regex or glob patterns. \
                     Returns matching component names with their source library paths. Supports both \
                     `.PcbLib` (footprints) and `.SchLib` (symbols) files."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepaths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Array of library file paths to search (.PcbLib or .SchLib)"
                        },
                        "pattern": {
                            "type": "string",
                            "description": "Search pattern to match component names"
                        },
                        "pattern_type": {
                            "type": "string",
                            "enum": ["glob", "regex"],
                            "description": "Pattern type: 'glob' (wildcards like * and ?) or 'regex' (regular expressions). Default: 'glob'"
                        }
                    },
                    "required": ["filepaths", "pattern"]
                }),
            },
            ToolDefinition {
                name: "get_component".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "get_component", "arguments": {"filepath": "./MyLibrary.PcbLib", "component_name": "SOIC-8"}})),
                description: Some(
                    "Get a single component by name from an Altium library. Returns the full component \
                     data (footprint or symbol) without needing to read and filter the entire library. \
                     Supports both `.PcbLib` (footprints) and `.SchLib` (symbols) files."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium library file (.PcbLib or .SchLib)"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Exact name of the component to retrieve"
                        }
                    },
                    "required": ["filepath", "component_name"]
                }),
            },
            ToolDefinition {
                name: "component_exists".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "component_exists", "arguments": {"filepath": "./MyLibrary.PcbLib", "component_names": ["RESC0603", "CAPC0402", "MISSING_COMPONENT"]}})),
                description: Some(
                    "Check if one or more components exist in an Altium library. Use this to validate \
                     component names before operations like rename, copy, or delete. Supports both \
                     `.PcbLib` and `.SchLib` files."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium library file (.PcbLib or .SchLib)"
                        },
                        "component_names": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of component names to check"
                        }
                    },
                    "required": ["filepath", "component_names"]
                }),
            },
            ToolDefinition {
                name: "render_footprint".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "render_footprint", "arguments": {"filepath": "./MyLibrary.PcbLib", "component_name": "RESC0603_IPC_MEDIUM", "scale": 2.0, "max_width": 80, "max_height": 40}})),
                description: Some(
                    "Render an ASCII art visualisation of a footprint from a PcbLib file: every \
                     primitive kind — pads (with designators), vias, tracks, arcs, fills, \
                     regions, text marks and 3D-body outlines — each with its own marker, plus \
                     a per-kind count line and a legend. A quick preview, not a rendering."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium PcbLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Name of the footprint to render"
                        },
                        "scale": {
                            "type": "number",
                            "description": "Characters per mm (default: 2.0). Higher = more detail"
                        },
                        "max_width": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Maximum width in characters (default: 80)"
                        },
                        "max_height": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Maximum height in characters (default: 40)"
                        }
                    },
                    "required": ["filepath", "component_name"]
                }),
            },
            ToolDefinition {
                name: "render_symbol".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "render_symbol", "arguments": {"filepath": "./MyLibrary.SchLib", "component_name": "LM358", "scale": 1.0, "max_width": 80, "max_height": 40, "part_id": 1}})),
                description: Some(
                    "Render an ASCII-art preview of one symbol in a .SchLib — a quick \
                     visual check of a layout after authoring or editing, not a drawing to \
                     keep. Every record kind of the requested part — pins (with \
                     designators), rectangles, rounded rectangles, lines, polylines, \
                     polygons, arcs, pies, ellipses, elliptical arcs, beziers, images, \
                     text frames, labels and IEEE symbols — gets its own marker, followed \
                     by a per-kind count line and a legend. part_id picks one part of a \
                     multi-part symbol (default 1; 0 draws all parts); scale sets \
                     characters per 10 schematic units and the picture is limited to \
                     max_width x max_height characters. Coordinates are schematic units \
                     (10 units = 1 grid). Use get_component or read_schlib for exact \
                     coordinates and properties, and render_footprint for a .PcbLib \
                     footprint."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium SchLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Name of the symbol to render"
                        },
                        "scale": {
                            "type": "number",
                            "description": "Characters per 10 schematic units (default: 1.0). Higher = more detail"
                        },
                        "max_width": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Maximum width in characters (default: 80)"
                        },
                        "max_height": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Maximum height in characters (default: 40)"
                        },
                        "part_id": {
                            "type": "integer",
                            "minimum": 0,
                            "description": "Part ID for multi-part symbols (default: 1, shows all parts if 0)"
                        }
                    },
                    "required": ["filepath", "component_name"]
                }),
            },
            // manage_schlib_parameters - Manage symbol parameters (Value, Manufacturer, etc.)
            ToolDefinition {
                name: "manage_schlib_parameters".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "manage_schlib_parameters", "arguments": {"filepath": "./MyLibrary.SchLib", "component_name": "LM358", "operation": "set", "parameter_name": "Value", "value": "LM358D"}})),
                description: Some(
                    "Manage the parameters of one symbol in a .SchLib (Value, \
                     Manufacturer, Part Number and the like): list them, get one, set an \
                     existing one, add a new one, or delete one. Names match without \
                     regard to case, as Altium treats them; set refuses a name the symbol \
                     lacks (use add) and add refuses one it already has (use set). Every \
                     change backs the library up to a timestamped .bak beside it (the five \
                     newest are kept) before saving, and the reply carries the parameter \
                     as stored; list returns every parameter with a count. Use \
                     get_component or read_schlib to see parameters together with the \
                     symbol's pins and graphics, update_component to rewrite a symbol \
                     wholesale, and manage_schlib_footprints for footprint links, which \
                     are models rather than parameters."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium SchLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Name of the symbol to manage parameters for"
                        },
                        "operation": {
                            "type": "string",
                            "enum": ["list", "get", "set", "add", "delete"],
                            "description": "Operation to perform: list (all parameters), get (single parameter), set (update value), add (new parameter), delete (remove parameter)"
                        },
                        "parameter_name": {
                            "type": "string",
                            "description": "Name of the parameter (required for get, set, add, delete); matched without regard to case, as Altium treats parameter names"
                        },
                        "value": {
                            "type": "string",
                            "description": "Parameter value (required for set, add)"
                        },
                        "hidden": {
                            "type": "boolean",
                            "description": "Whether the parameter is hidden (optional for set, add)"
                        },
                        "read_only_state": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 1,
                            "description": "Read-only state (0=editable, 1=read-only) (optional for set, add). Default: 0"
                        },
                        "param_type": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 3,
                            "description": "Parameter type (0=String, 1=Boolean, 2=Integer, 3=Float) (optional for set, add). Default: 0"
                        },
                        "unique_id": {
                            "type": "string",
                            "description": "8-char Altium unique ID (optional for set, add). Default: auto-generated"
                        },
                        "x": {
                            "type": "integer",
                            "description": "X position in schematic units (optional for set, add)"
                        },
                        "y": {
                            "type": "integer",
                            "description": "Y position in schematic units (optional for set, add)"
                        }
                    },
                    "required": ["filepath", "component_name", "operation"]
                }),
            },
            // manage_schlib_footprints - Manage footprint links in symbols
            ToolDefinition {
                name: "manage_schlib_footprints".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "manage_schlib_footprints", "arguments": {"filepath": "./MyLibrary.SchLib", "component_name": "LM358", "operation": "add", "footprint_name": "SOIC-8_3.9x4.9mm"}})),
                description: Some(
                    "Manage the footprint links (PCB models) of one symbol in a .SchLib: \
                     list them, add one, or remove one. A link names a .PcbLib footprint; \
                     add refuses a name already linked and remove refuses one that is not \
                     (names match without regard to case), and neither touches the .PcbLib \
                     itself. Give library_path on add when Altium should resolve and \
                     preview the footprint from a specific library; omit it to link by \
                     name only. Every change backs the library up to a timestamped .bak \
                     beside it (the five newest are kept) before saving. Use \
                     manage_schlib_parameters for parameters such as Value, get_component \
                     or read_schlib to see the links together with the rest of the symbol, \
                     and update_component to rewrite a symbol wholesale."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the Altium SchLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Name of the symbol to manage footprints for"
                        },
                        "operation": {
                            "type": "string",
                            "enum": ["list", "add", "remove"],
                            "description": "Operation to perform: list (all footprints), add (new footprint link), remove (delete footprint link)"
                        },
                        "footprint_name": {
                            "type": "string",
                            "description": "Footprint name (required for add, remove)"
                        },
                        "description": {
                            "type": "string",
                            "description": "Footprint description (optional for add). Keep to 256 characters if the library will be imported into an Altium 365 workspace — that importer refuses longer ones; a longer description is written and reported as a validation warning."
                        },
                        "library_path": {
                            "type": "string",
                            "description": "Optional (add): absolute path to the .PcbLib containing the footprint, written as ModelDatafile0 so Altium can resolve and preview the model. Omit to link by name only (requires the library to be installed/in the project, else 'footprint not found')."
                        }
                    },
                    "required": ["filepath", "component_name", "operation"]
                }),
            },
            ToolDefinition {
                name: "compare_components".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "compare_components", "arguments": {"filepath_a": "./LibraryA.PcbLib", "component_a": "RESC0603_V1", "filepath_b": "./LibraryB.PcbLib", "component_b": "RESC0603_V2", "include_geometry": true, "tolerance": 0.001}})),
                description: Some(
                    "Compare two named components — from one library or two of the same \
                     type (.PcbLib with .PcbLib, .SchLib with .SchLib) — and report every \
                     difference: description and parameters, then per primitive kind: \
                     pads, vias, tracks, arcs, regions, text, fills and 3D bodies of a \
                     footprint; pins, every graphic shape, parameters and footprint links \
                     of a symbol. Geometry compares within tolerance (mm, default 0.001) \
                     and unmatched items are reported per side; include_geometry false \
                     compares counts and properties only. Identity (GUIDs, unique ids) is \
                     never a difference. Read-only; returns identical, difference_count, \
                     the differences and per-kind count summaries. Use diff_libraries \
                     first to find which components differ between two whole libraries, \
                     then this tool on one pair; use get_component for one component's \
                     full data rather than a comparison."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath_a": {
                            "type": "string",
                            "description": "Path to the first library file (.PcbLib or .SchLib)"
                        },
                        "component_a": {
                            "type": "string",
                            "description": "Name of the first component"
                        },
                        "filepath_b": {
                            "type": "string",
                            "description": "Path to the second library file (can be same as filepath_a)"
                        },
                        "component_b": {
                            "type": "string",
                            "description": "Name of the second component"
                        },
                        "include_geometry": {
                            "type": "boolean",
                            "description": "Include detailed geometry comparisons for primitives (default: true)"
                        },
                        "tolerance": {
                            "type": "number",
                            "description": "Tolerance for floating-point comparisons in mm (default: 0.001)"
                        }
                    },
                    "required": ["filepath_a", "component_a", "filepath_b", "component_b"]
                }),
            },
            ToolDefinition {
                name: "repair_library".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "repair_library", "arguments": {"filepath": "./MyLibrary.PcbLib", "dry_run": true}})),
                description: Some(
                    "Repair a PcbLib by removing orphaned 3D-model data: \
                     (1) embedded models not referenced by any footprint, and \
                     (2) component body references that point to non-existent models. \
                     This fixes libraries where STEP model data is missing but references remain \
                     (validate_library reports both conditions). PcbLib only; a SchLib is refused."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the library file (.PcbLib)"
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "If true, report what would be fixed without making changes (default: false)"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            ToolDefinition {
                name: "list_backups".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "list_backups", "arguments": {"filepath": "./MyLibrary.PcbLib"}})),
                description: Some(
                    "List available backup files for an Altium library. Shows timestamped .bak files \
                     that were automatically created before write operations."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the library file (.PcbLib or .SchLib)"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            ToolDefinition {
                name: "restore_backup".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "restore_backup", "arguments": {"filepath": "./MyLibrary.PcbLib", "backup_path": "MyLibrary.PcbLib.20260125_091500.bak"}})),
                description: Some(
                    "Restore an Altium library file from a backup. If no specific backup is specified, \
                     restores from the most recent backup. The current file is snapshotted as a new \
                     backup first (reported as pre_restore_backup), so a wrong pick is itself \
                     reversible, and the restore is written atomically."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the library file to restore"
                        },
                        "backup_path": {
                            "type": "string",
                            "description": "Optional: specific backup file to restore from. If not provided, uses most recent backup."
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            ToolDefinition {
                name: "bulk_rename".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "bulk_rename", "arguments": {"filepath": "./MyLibrary.PcbLib", "pattern": "^RESC(.*)$", "replacement": "RES_$1", "dry_run": true}})),
                description: Some(
                    "Rename multiple components in a library using regex pattern matching. \
                     Supports capture groups for flexible renaming (e.g., 'RESC(.*)' -> 'RES_$1')."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the library file (.PcbLib or .SchLib)"
                        },
                        "pattern": {
                            "type": "string",
                            "description": "Regex pattern to match component names (e.g., '^RESC(.*)$')"
                        },
                        "replacement": {
                            "type": "string",
                            "description": "Replacement string with optional capture groups (e.g., 'RES_$1')"
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "If true, show what would be renamed without making changes (default: false)"
                        }
                    },
                    "required": ["filepath", "pattern", "replacement"]
                }),
            },
            ToolDefinition {
                name: "update_pad".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "update_pad", "arguments": {"filepath": "./MyLibrary.PcbLib", "component_name": "RESC0603", "designator": "1", "updates": {"width": 1.0, "height": 0.9, "shape": "rectangle"}, "dry_run": false}})),
                description: Some(
                    "Update specific properties of a pad in a PcbLib footprint without replacing \
                     the entire component. Find pad by designator and apply only the specified updates. \
                     On a stacked pad (stack_mode other than simple) a width/height/shape change also \
                     reaches the per-layer tables: layers that shared the old primary value follow it, \
                     layers with their own value keep it, and the response reports how many followed."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Name of the footprint containing the pad"
                        },
                        "designator": {
                            "type": "string",
                            "description": "Pad designator (e.g., '1', '2', 'A1')"
                        },
                        "updates": {
                            "type": "object",
                            "description": "Properties to update (only specified properties are changed)",
                            "properties": {
                                "x": { "type": "number", "description": "New X position in mm" },
                                "y": { "type": "number", "description": "New Y position in mm" },
                                "width": { "type": "number", "description": "New width in mm" },
                                "height": { "type": "number", "description": "New height in mm" },
                                "shape": { "type": "string", "enum": ["rectangle", "rounded_rectangle", "round", "circle", "oval", "octagonal"], "description": "New pad shape. Same vocabulary as write_pcblib: rectangle, rounded_rectangle, round/circle, oval, octagonal. Matching is case-insensitive and ignores '_'/'-'." },
                                "rotation": { "type": "number", "description": "New rotation in degrees" },
                                "hole_size": { "type": "number", "description": "New hole diameter for through-hole pads" }
                            }
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "If true, show what would change without saving (default: false)"
                        }
                    },
                    "required": ["filepath", "component_name", "designator", "updates"]
                }),
            },
            ToolDefinition {
                name: "update_primitive".to_string(),
                title: None,
                annotations: None,
                example: Some(serde_json::json!({"name": "update_primitive", "arguments": {"filepath": "./MyLibrary.PcbLib", "component_name": "RESC0603", "primitive_type": "track", "index": 0, "updates": {"width": 0.15, "layer": "Top Overlay"}, "dry_run": false}})),
                description: Some(
                    "Update specific properties of a primitive (track, arc, text, fill, region or \
                     via) in a PcbLib footprint. Find the primitive by type and index (its position \
                     in read_pcblib's list for that type), apply only the specified updates. \
                     Moving a primitive to another layer drops the layer carriers it was read \
                     with (a region's v7_layer, an unmapped raw_layer_id) so the new layer's own \
                     token is written. On a stacked via (diameter_stack_mode other than simple) \
                     a diameter change also reaches per_layer_diameters: layers that shared the \
                     old diameter follow it, layers with their own value keep it, and the \
                     response reports how many followed."
                        .to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "filepath": {
                            "type": "string",
                            "description": "Path to the .PcbLib file"
                        },
                        "component_name": {
                            "type": "string",
                            "description": "Name of the footprint containing the primitive"
                        },
                        "primitive_type": {
                            "type": "string",
                            "enum": UPDATE_PRIMITIVE_KINDS,
                            "description": "Type of primitive to update. Addressed by `index` into that primitive list. Pads are not here — they have a designator, so use update_pad."
                        },
                        "index": {
                            "type": "integer",
                            "minimum": 0,
                            "description": "Zero-based index of the primitive within its type array"
                        },
                        "updates": {
                            "type": "object",
                            "description": "Properties to update (only specified properties are changed). Valid properties depend on primitive_type — track: x1, y1, x2, y2, width, layer; arc: x/x1, y/y1, radius, start_angle, end_angle, width, layer; text: x, y, height, rotation, text, layer; fill: x/x1, y/y1, x2, y2, rotation, layer; region: layer; via: x, y, diameter, hole_size, from_layer, to_layer. Any other key is refused.",
                            "properties": {
                                "x1": { "type": "number", "description": "Start X (track) or centre X (arc)" },
                                "y1": { "type": "number", "description": "Start Y (track) or centre Y (arc)" },
                                "x2": { "type": "number", "description": "End X (track)" },
                                "y2": { "type": "number", "description": "End Y (track)" },
                                "x": { "type": "number", "description": "X position (text, fill)" },
                                "y": { "type": "number", "description": "Y position (text, fill)" },
                                "width": { "type": "number", "description": "Line width (track, arc) or width (fill)" },
                                "height": { "type": "number", "description": "Height (text, fill)" },
                                "radius": { "type": "number", "description": "Radius (arc)" },
                                "start_angle": { "type": "number", "description": "Start angle in degrees (arc)" },
                                "end_angle": { "type": "number", "description": "End angle in degrees (arc)" },
                                "text": { "type": "string", "description": "Text content (text primitive)" },
                                "rotation": { "type": "number", "description": "Rotation angle (text, fill)" },
                                "layer": { "type": "string", "description": "Layer name (track, arc, text, fill, region)" },
                                "diameter": { "type": "number", "description": "Barrel diameter in mm (via)" },
                                "hole_size": { "type": "number", "description": "Hole diameter in mm (via)" },
                                "from_layer": { "type": "string", "description": "Start layer (via)" },
                                "to_layer": { "type": "string", "description": "End layer (via)" }
                            }
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "If true, show what would change without saving (default: false)"
                        }
                    },
                    "required": ["filepath", "component_name", "primitive_type", "index", "updates"]
                }),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Recursively collect the paths of JSON Schema nodes that declare
    /// `"type": "array"` without the required `items` keyword.
    fn arrays_missing_items(node: &Value, path: &str, out: &mut Vec<String>) {
        match node {
            Value::Object(map) => {
                if map.get("type").and_then(Value::as_str) == Some("array")
                    && !map.contains_key("items")
                {
                    out.push(path.to_string());
                }
                for (key, value) in map {
                    arrays_missing_items(value, &format!("{path}.{key}"), out);
                }
            }
            Value::Array(items) => {
                for (i, value) in items.iter().enumerate() {
                    arrays_missing_items(value, &format!("{path}[{i}]"), out);
                }
            }
            _ => {}
        }
    }

    /// Every array property in every tool schema must declare `items`. Strict
    /// JSON Schema validators — notably Google's Gemini, which backs several MCP
    /// clients — reject an array without `items` and refuse to load the whole
    /// server (issue #70). This guards against re-introducing such a schema.
    #[test]
    fn every_tool_array_property_declares_items() {
        let mut violations = Vec::new();
        for tool in McpServer::get_tool_definitions() {
            arrays_missing_items(&tool.input_schema, &tool.name, &mut violations);
        }
        assert!(
            violations.is_empty(),
            "array schema(s) missing `items` (rejected by Gemini/strict validators): {violations:?}"
        );
    }
}
