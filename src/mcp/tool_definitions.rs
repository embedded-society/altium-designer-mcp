//! Tool definitions for the MCP `tools/list` response.
//!
//! Extracted from `server.rs` to keep that file navigable. This is purely the
//! static tool schema (names, descriptions, JSON input schemas); it carries no
//! behaviour, so it lives apart from the request handlers.

use serde_json::json;

use crate::mcp::server::{McpServer, ToolDefinition};

impl McpServer {
    /// Returns the list of available tools.
    ///
    /// These are low-level file I/O and primitive placement tools.
    /// The AI handles IPC calculations and design decisions.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn get_tool_definitions() -> Vec<ToolDefinition> {
        vec![
            // === Library Reading ===
            ToolDefinition {
                name: "read_pcblib".to_string(),
                example: Some(serde_json::json!({"name": "read_pcblib", "arguments": {"filepath": "./MyLibrary.PcbLib"}})),
                description: Some(
                    "Read an Altium .PcbLib file and return its contents including footprints \
                     with their primitives (pads, tracks, arcs, regions, text). Returns structured \
                     data that can be used to understand existing footprint styles. \
                     All coordinates and dimensions are in millimetres (mm). \
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
                            "description": "Optional: fetch only this specific footprint by name"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Optional: maximum number of footprints to return (default: all)"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Optional: skip first N footprints (default: 0)"
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
                example: Some(serde_json::json!({"name": "read_schlib", "arguments": {"filepath": "./MySymbols.SchLib"}})),
                description: Some(
                    "Read an Altium .SchLib file and return its contents including symbols \
                     with their primitives (pins, rectangles, lines, text). \
                     Coordinates are in schematic units (10 units = 1 grid square, not mm). \
                     For large libraries, use component_name to fetch specific symbols, \
                     or use limit/offset for pagination."
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
                            "description": "Optional: fetch only this specific symbol by name"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Optional: maximum number of symbols to return (default: all)"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Optional: skip first N symbols (default: 0)"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            ToolDefinition {
                name: "list_components".to_string(),
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
                            "description": "Maximum number of components to return (optional, default: all)"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Number of components to skip (optional, default: 0)"
                        },
                        "include_metadata": {
                            "type": "boolean",
                            "description": "If true, return objects with metadata (part_count, pin_count, etc.) instead of just names. Default: false"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            // === Style Extraction ===
            ToolDefinition {
                name: "extract_style".to_string(),
                example: Some(serde_json::json!({"name": "extract_style", "arguments": {"filepath": "./MyLibrary.PcbLib"}})),
                description: Some(
                    "Extract style information from an existing Altium library file. Returns \
                     statistics about track widths, colours, pin lengths, layer usage, and other \
                     styling parameters. Use this to learn from existing libraries and create \
                     consistent new components."
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
            // === Library Writing ===
            ToolDefinition {
                name: "write_pcblib".to_string(),
                example: Some(serde_json::json!({"name": "write_pcblib", "arguments": {"filepath": "./Passives.PcbLib", "footprints": [{"name": "RESC1608X55N", "description": "Chip resistor, 0603 (1608 metric)", "pads": [{"designator": "1", "x": -0.75, "y": 0, "width": 0.9, "height": 0.95}, {"designator": "2", "x": 0.75, "y": 0, "width": 0.9, "height": 0.95}], "tracks": [{"x1": -0.8, "y1": -0.425, "x2": 0.8, "y2": -0.425, "width": 0.12, "layer": "Top Overlay"}, {"x1": -0.8, "y1": 0.425, "x2": 0.8, "y2": 0.425, "width": 0.12, "layer": "Top Overlay"}], "regions": [{"vertices": [{"x": -1.45, "y": -0.73}, {"x": 1.45, "y": -0.73}, {"x": 1.45, "y": 0.73}, {"x": -1.45, "y": 0.73}], "layer": "Top Courtyard"}]}], "append": false}})),
                description: Some(
                    "Write footprints to an Altium .PcbLib file. Each footprint is defined by \
                     its primitives: pads (with position, size, shape, layer), tracks, vias, \
                     fills, arcs, regions, and text. The AI is responsible for calculating correct positions \
                     and sizes based on IPC-7351B or other standards. \
                     All coordinates and dimensions must be in millimetres (mm). \
                     The response 'bodies' array echoes each footprint's 3D body height and source; \
                     a footprint with no STEP model and no component body reports source 'none'. \
                     Set 'auto_3d_body': true to have an extruded placeholder body (default height \
                     1.0 mm, flagged 'assumed_height': true) added to such footprints, then confirm \
                     or override it by supplying 'component_bodies' explicitly. The response also includes a \
                     'warnings' array flagging silkscreen (overlay) tracks that overlap a pad \
                     (silk-on-pad) so you can move them clear."
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
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": {
                                        "type": "string",
                                        "description": "Footprint name (e.g., 'RESC1608X55N')"
                                    },
                                    "description": {
                                        "type": "string",
                                        "description": "Footprint description"
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
                                                    "description": "Pad shape: rectangle (pin 1), rounded_rectangle (SMD default), round/circle (equivalent, for through-hole), oval, octagonal"
                                                },
                                                "layer": { "type": "string", "description": "Layer name: Top Layer, Bottom Layer, Multi-Layer (default for SMD)" },
                                                "hole_size": { "type": "number", "description": "Hole diameter for through-hole pads (mm)" },
                                                "hole_shape": {
                                                    "type": "string",
                                                    "enum": ["round", "square", "slot"],
                                                    "description": "Drill hole shape. Default: round. Use slot for oblong holes (set hole_slot_length)"
                                                },
                                                "hole_slot_length": { "type": "number", "description": "Slot length in mm for a slot hole (hole_shape=slot). Default: 0" },
                                                "hole_rotation": { "type": "number", "description": "Hole rotation in degrees (rotates a slot hole). Default: 0" },
                                                "hole_positive_tolerance": { "type": "number", "description": "Positive drill tolerance in mm (optional; omit to leave unset)" },
                                                "hole_negative_tolerance": { "type": "number", "description": "Negative drill tolerance in mm (optional; omit to leave unset)" },
                                                "solder_mask_expansion": { "type": "number", "description": "Solder mask expansion in mm (optional; omit to use the rule default)" },
                                                "solder_mask_expansion_mode": {
                                                    "type": "string",
                                                    "enum": ["none", "from_rule", "manual"],
                                                    "description": "Solder mask expansion mode. Default: from_rule"
                                                },
                                                "power_plane_connect_style": {
                                                    "type": "string",
                                                    "enum": ["relief", "direct", "no_connect"],
                                                    "description": "How the pad connects to an internal power plane. Default: relief (thermal spokes)"
                                                },
                                                "relief_conductor_width": { "type": "number", "description": "Thermal-relief spoke (conductor) width in mm. Default: 0.254 (10 mil)" },
                                                "relief_entries": { "type": "integer", "description": "Number of thermal-relief spokes. Default: 4" },
                                                "relief_air_gap": { "type": "number", "description": "Thermal-relief air-gap width in mm. Default: 0.254 (10 mil)" },
                                                "power_plane_relief_expansion": { "type": "number", "description": "Power-plane relief expansion in mm. Default: 0.508 (20 mil)" },
                                                "power_plane_clearance": { "type": "number", "description": "Power-plane (anti-pad) clearance to the plane in mm. Default: 0.508 (20 mil)" },
                                                "net_index": { "type": "integer", "description": "Net index into the board net list (common header, 0-65534; 65535 = no net). Normally omitted for library footprints; preserved on a read-modify-write. Default: 65535" },
                                                "polygon_index": { "type": "integer", "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                                                "component_index": { "type": "integer", "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                                                "flags": { "type": ["string", "integer"], "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"LOCKED\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom). Default: none" }
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
                                                "keepout_restrictions": { "type": "integer", "description": "Keepout restriction bitmask (optional; defaults to 0)" },
                                                "net_index": { "type": "integer", "description": "Net index into the board net list (common header, 0-65534; 65535 = no net). Normally omitted for library footprints; preserved on a read-modify-write. Default: 65535" },
                                                "polygon_index": { "type": "integer", "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                                                "component_index": { "type": "integer", "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                                                "flags": { "type": ["string", "integer"], "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"LOCKED\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom). Default: none" }
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
                                                    "enum": ["none", "from_rule", "manual"],
                                                    "description": "Solder mask expansion mode. Default: from_rule"
                                                },
                                                "thermal_relief_gap": { "type": "number", "description": "Thermal relief air-gap width in mm. Default: 0.254 (10 mil)" },
                                                "thermal_relief_conductors": { "type": "integer", "description": "Number of thermal relief conductors. Default: 4" },
                                                "thermal_relief_width": { "type": "number", "description": "Thermal relief conductor width in mm. Default: 0.254 (10 mil)" },
                                                "power_plane_connect_style": {
                                                    "type": "string",
                                                    "enum": ["relief", "direct", "no_connect"],
                                                    "description": "How the via connects to an internal power plane. Default: relief (thermal spokes)"
                                                },
                                                "power_plane_relief_expansion": { "type": "number", "description": "Power-plane relief expansion in mm. Default: 0.508 (20 mil)" },
                                                "power_plane_clearance": { "type": "number", "description": "Power-plane (anti-pad) clearance in mm. Default: 0.508 (20 mil)" },
                                                "paste_mask_expansion": { "type": "number", "description": "Paste-mask expansion in mm. Default: 0" },
                                                "net_index": { "type": "integer", "description": "Net index into the board net list (0-65534; 65535 = no net). Default: 65535" },
                                                "polygon_index": { "type": "integer", "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                                                "component_index": { "type": "integer", "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                                                "hole_positive_tolerance": { "type": "number", "description": "Positive drill tolerance in mm (optional; omit to leave unset)" },
                                                "hole_negative_tolerance": { "type": "number", "description": "Negative drill tolerance in mm (optional; omit to leave unset)" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                                                "flags": { "type": ["string", "integer"], "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"TENTING_TOP\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom). Tenting covers the via with solder mask. Default: none" }
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
                                                "keepout_restrictions": { "type": "integer", "description": "Keepout restriction bitmask (optional; defaults to 0)" },
                                                "net_index": { "type": "integer", "description": "Net index into the board net list (common header, 0-65534; 65535 = no net). Normally omitted for library footprints; preserved on a read-modify-write. Default: 65535" },
                                                "polygon_index": { "type": "integer", "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                                                "component_index": { "type": "integer", "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                                                "flags": { "type": ["string", "integer"], "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"LOCKED\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom). Default: none" }
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
                                                "keepout_restrictions": { "type": "integer", "description": "Keepout restriction bitmask (optional; defaults to 0)" },
                                                "net_index": { "type": "integer", "description": "Net index into the board net list (common header, 0-65534; 65535 = no net). Normally omitted for library footprints; preserved on a read-modify-write. Default: 65535" },
                                                "polygon_index": { "type": "integer", "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                                                "component_index": { "type": "integer", "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                                                "flags": { "type": ["string", "integer"], "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"LOCKED\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom). Default: none" }
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
                                                "kind": { "type": ["string", "integer"], "description": "Region kind (optional). \"copper\" (default) for a copper pour/fill, \"cutout\" for a board/polygon cutout, or a raw Altium KIND integer. Default: copper" },
                                                "name": { "type": "string", "description": "Region name (the NAME parameter, optional). Default: empty" },
                                                "net_index": { "type": "integer", "description": "Net index into the board net list (optional). 65535 = no net. Default: 65535" },
                                                "polygon_index": { "type": "integer", "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                                                "component_index": { "type": "integer", "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                                                "cavity_height": { "type": "number", "description": "Cavity height in mm for embedded components (optional). Default: 0" },
                                                "arc_resolution": { "type": "number", "description": "Altium ARCRESOLUTION (arc-to-line tolerance, optional). Normally omitted; preserved on a read-modify-write. Default: 0" },
                                                "sub_poly_index": { "type": "integer", "description": "Altium SUBPOLYINDEX; -1 when not a polygon sub-shape. Preserved on a read-modify-write. Default: -1" },
                                                "union_index": { "type": "integer", "description": "Altium UNIONINDEX for grouped primitives. Preserved on a read-modify-write. Default: 0" },
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
                                                "flags": { "type": ["string", "integer"], "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"LOCKED\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom). Default: none" }
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
                                                "kind": { "type": "string", "enum": ["stroke", "true_type", "bar_code"], "description": "Text rendering kind. \"stroke\" (default) uses a vector stroke font (most common for silkscreen); \"true_type\" renders with the TrueType font named by font_name; \"bar_code\" is a barcode. Default: stroke" },
                                                "stroke_font": { "type": "string", "enum": ["default", "sans_serif", "serif"], "description": "Stroke font selection (only meaningful when kind is \"stroke\"). Default: default (Altium's built-in stroke font)" },
                                                "font_name": { "type": "string", "description": "TrueType font name (only meaningful when kind is \"true_type\"). Default: Arial" },
                                                "bold": { "type": "boolean", "description": "Bold font style (TrueType). Default: false" },
                                                "italic": { "type": "boolean", "description": "Italic font style (TrueType). Default: false" },
                                                "mirror": { "type": "boolean", "description": "Mirror the text (bottom-side silkscreen). Default: false" },
                                                "justification": { "type": "string", "enum": ["bottom_left", "bottom_center", "bottom_right", "middle_left", "middle_center", "middle_right", "top_left", "top_center", "top_right"], "description": "Text anchor / justification within its frame. Default: bottom_left" },
                                                "stroke_width": { "type": "number", "description": "Stroke line width in mm (optional; defaults to Altium's ~4 mil)" },
                                                "is_inverted": { "type": "boolean", "description": "Draw the text inverted (knockout): a filled bar with the glyphs punched out. Default: false" },
                                                "inverted_border": { "type": "number", "description": "Border margin around inverted text in mm (only meaningful when is_inverted). Default: none" },
                                                "use_inverted_rectangle": { "type": "boolean", "description": "Use an explicit framed rectangle (inverted_rect_width / inverted_rect_height) for the inverted text box instead of auto-sizing to the glyphs. Default: false" },
                                                "inverted_rect_width": { "type": "number", "description": "Inverted-rectangle width in mm (only meaningful when use_inverted_rectangle). Default: none" },
                                                "inverted_rect_height": { "type": "number", "description": "Inverted-rectangle height in mm (only meaningful when use_inverted_rectangle). Default: none" },
                                                "inverted_rect_text_offset": { "type": "number", "description": "Text offset within the inverted rectangle in mm (only meaningful when use_inverted_rectangle). Default: none" },
                                                "net_index": { "type": "integer", "description": "Net index into the board net list (common header, 0-65534; 65535 = no net). Normally omitted for library footprints; preserved on a read-modify-write. Default: 65535" },
                                                "polygon_index": { "type": "integer", "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                                                "component_index": { "type": "integer", "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                                                "flags": { "type": ["string", "integer"], "description": "Primitive flags (optional). Accepts the name string read_pcblib emits (e.g. \"LOCKED\" or \"LOCKED | KEEPOUT\") or a raw bitmask integer (1=locked, 2=polygon, 4=keepout, 8=tenting-top, 16=tenting-bottom). Default: none" }
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
                                    "component_bodies": {
                                        "type": "array",
                                        "description": "Generic extruded 3D bodies (no STEP file). Each is an extruded shape defined by an outline + heights, useful for giving parts a 3D height when no STEP model is available.",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "overall_height": { "type": "number", "description": "Total body height above the board, in mm (top of extrusion)" },
                                                "standoff_height": { "type": "number", "description": "Standoff from the board to the bottom of the body, in mm. Default: 0" },
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
                                                "kind": { "type": "integer", "description": "Altium KIND (0=extruded, etc.). Default: 0" },
                                                "sub_poly_index": { "type": "integer", "description": "Altium SUBPOLYINDEX; -1 when not a polygon sub-shape. Default: -1" },
                                                "union_index": { "type": "integer", "description": "Altium UNIONINDEX for grouped primitives. Default: 0" },
                                                "is_shape_based": { "type": "boolean", "description": "Altium ISSHAPEBASED (shape-based vs. model-based body). Default: false" },
                                                "body_projection": { "type": "integer", "description": "Altium BODYPROJECTION (board side). Default: 0" },
                                                "body_color_3d": { "type": "integer", "description": "3D body colour as decimal RGB (Altium BODYCOLOR3D). Default: 8421504 (0xE0E0E0, grey)" },
                                                "body_opacity_3d": { "type": "number", "description": "3D body opacity, 0.0-1.0 (Altium BODYOPACITY3D). Default: 1.0" },
                                                "model_2d_rotation": { "type": "number", "description": "2D placement rotation in degrees (Altium MODEL.2D.ROTATION). Default: 0" },
                                                "model_id": { "type": "string", "description": "Model GUID referencing an embedded model (Altium MODELID). Default: \"\" (none)" },
                                                "model_name": { "type": "string", "description": "Model filename or external path (Altium MODEL.NAME). Default: \"\" (none)" },
                                                "embedded": { "type": "boolean", "description": "Whether the model is embedded in the library (Altium MODEL.EMBED). Default: false" },
                                                "net_index": { "type": "integer", "description": "Net index into the board net list (common header, 0-65534; 65535 = no net). Normally omitted for library footprints; preserved on a read-modify-write. Default: 65535" },
                                                "polygon_index": { "type": "integer", "description": "Polygon index (common header; 65535 = none). Normally omitted; preserved on a read-modify-write. Default: 65535" },
                                                "component_index": { "type": "integer", "description": "Component index into the board component list (common header; -1 = free primitive). Normally omitted; preserved on a read-modify-write. Default: -1" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" },
                                                "additional_parameters": { "type": "array", "description": "Unmodelled body parameter keys captured verbatim on read (e.g. TEXTURE, MODEL.2D.X, IDENTIFIER, MODEL.MODELTYPE, MODEL.MODELSOURCE, the extrusion range). Each entry is a [key, value] string pair. Round-tripped so a read-modify-write does not drop keys the tool does not model. Normally omitted; supply only the pairs read_pcblib returned.", "items": { "type": "array", "items": { "type": "string" }, "minItems": 2, "maxItems": 2 } }
                                            },
                                            "required": ["overall_height"]
                                        }
                                    }
                                },
                                "required": ["name", "pads"]
                            }
                        },
                        "append": {
                            "type": "boolean",
                            "description": "If true, append to existing file; if false, create new file"
                        },
                        "auto_3d_body": {
                            "type": "boolean",
                            "description": "If true, footprints with pads but no STEP model and no component body get a placeholder extruded 3D body (1.0 mm tall, flagged assumed_height). Default false: nothing is added unless you ask, since many footprints (fiducials, test points, mounting holes) legitimately have no body. Prefer supplying real heights via component_bodies."
                        }
                    },
                    "required": ["filepath", "footprints"]
                }),
            },
            ToolDefinition {
                name: "write_schlib".to_string(),
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
                    "Write schematic symbols to an Altium .SchLib file. Each symbol is defined by \
                     its primitives: pins, rectangles, round_rects, lines, polylines, polygons, \
                     arcs, ellipses, labels, and text. \
                     Coordinates must be in schematic units (10 units = 1 grid square, not mm)."
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
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string" },
                                    "description": { "type": "string" },
                                    "designator_prefix": { "type": "string", "description": "Reference-designator class letter, e.g. 'R' for resistors, 'U' for ICs. Written as '<prefix>?'. If omitted, falls back to 'component_type' (IEEE 315 / ASME Y14.44 mapping), then to 'U'." },
                                    "component_type": { "type": "string", "description": "Optional component category (e.g. 'resistor', 'capacitor', 'inductor', 'diode', 'transistor', 'connector', 'crystal', 'ic') used to derive the IEEE designator letter when 'designator_prefix' is not given. Unknown values default to 'U'." },
                                    "part_count": { "type": "integer", "description": "Number of parts for multi-part symbols (e.g., 2 for dual op-amp). Default: 1" },
                                    "pins": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "designator": { "type": "string" },
                                                "name": { "type": "string" },
                                                "x": { "type": "number", "description": "Pin's body-attach (INNER) end, in schematic units (10 units = 1 grid square). This is the end that touches the symbol body, NOT the connection tip. The pin is drawn from (x,y) extending 'length' units in the 'orientation' direction; the connection tip is at the far end." },
                                                "y": { "type": "number", "description": "Y of the pin's body-attach (inner) end, in schematic units. See 'x'." },
                                                "length": { "type": "number", "description": "Pin length in schematic units (10 = 1 grid). Drawn from (x,y) outward in the 'orientation' direction." },
                                                "orientation": { "type": "string", "enum": ["left", "right", "up", "down"], "description": "Direction the pin POINTS, away from the body — NOT which side it sits on. A pin on the LEFT side uses 'left' (tip at x-length); a RIGHT-side pin uses 'right' (tip at x+length); 'up'/'down' for top/bottom pins. Put each pin's (x,y) on the matching body-rectangle edge so it attaches flush, e.g. left pin {x:-50,y:20,length:30,orientation:'left'} with rectangle x1=-50, and the matching right pin {x:50,y:20,length:30,orientation:'right'} with x2=50. For TOP/BOTTOM pins, (x,y) sits on the body's top/bottom edge and the pin points outward (away from the body centre): a top-side pin uses 'up' (tip at y+length, above the body), a bottom-side pin uses 'down' (tip at y-length, below) — e.g. a vertical 2-pin part with the body near y=0: top pin {x:0,y:10,length:30,orientation:'up'} (tip at y=40), bottom pin {x:0,y:-10,length:30,orientation:'down'} (tip at y=-40)." },
                                                "electrical_type": { "type": "string", "enum": ["input", "output", "bidirectional", "passive", "power", "open_collector", "open_emitter", "hi_z", "tristate"], "description": "Pin electrical type. 'tristate' is accepted as an alias for 'hi_z'. Default: passive" },
                                                "owner_part_id": { "type": "integer", "description": "Part number this pin belongs to (1-based). Default: 1" },
                                                "hidden": { "type": "boolean", "description": "Whether the pin is hidden. Default: false" },
                                                "show_name": { "type": "boolean", "description": "Whether to show the pin name. Default: true" },
                                                "show_designator": { "type": "boolean", "description": "Whether to show the pin designator. Default: true" },
                                                "description": { "type": "string", "description": "Pin description. Default: empty" },
                                                "colour": { "type": "integer", "description": "Pin colour (BGR integer). Default: 0" },
                                                "graphically_locked": { "type": "boolean", "description": "Whether the pin is graphically locked. Default: false" },
                                                "swap_id_group": { "type": "string", "description": "Pin swap-id group, for pin-swap. Default: empty" },
                                                "part_and_sequence": { "type": "string", "description": "Pin part-and-sequence swap id. Default: '|&|'" },
                                                "default_value": { "type": "string", "description": "Pin default value. Default: empty" },
                                                "symbol_inner_edge": { "type": "string", "description": "Decoration on the INNER edge (nearest the body), e.g. 'dot' (inversion bubble), 'clock'. Default: none" },
                                                "symbol_outer_edge": { "type": "string", "description": "Decoration on the OUTER edge (furthest from the body), e.g. 'dot', 'clock'. Default: none" },
                                                "symbol_inside": { "type": "string", "description": "Decoration drawn inside the pin line, e.g. 'postponed_output', 'open_collector'. Default: none" },
                                                "symbol_outside": { "type": "string", "description": "Decoration drawn outside the pin line, e.g. 'right_left_signal_flow', 'analog_signal_in'. Default: none" },
                                                "owner_part_display_mode": { "type": "integer", "description": "Pin's alternate-view (display-mode) index in the binary pin record. Default: 0" },
                                                "symbol_line_width": { "type": "integer", "description": "Pin symbol line-width index. Non-zero writes a PinSymbolLineWidth auxiliary stream; 0 (default) writes none." },
                                                "frac": { "type": "object", "description": "Fractional pin coordinates for off-grid pins, in 1/100000 schematic-unit steps. Non-zero writes a PinFrac auxiliary stream; omit for on-grid pins.", "properties": { "x": { "type": "integer" }, "y": { "type": "integer" }, "length": { "type": "integer" } } }
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
                                                "line_width": { "type": "integer", "description": "Border width. Default: 1" },
                                                "line_color": { "type": "integer", "description": "Border BGR colour. Default: 0x000080" },
                                                "fill_color": { "type": "integer", "description": "Fill BGR colour. Default: 0xB0FFFF (Altium light yellow)" },
                                                "line_style": { "type": "integer", "description": "Border line style: 0=Solid, 1=Dashed, 2=Dotted. Default: 0" },
                                                "filled": { "type": "boolean", "description": "Whether filled. Default: true" },
                                                "transparent": { "type": "boolean", "description": "Whether the fill is transparent. Default: false" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" },
                                                "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                                                "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                                                "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                                                "owner_part_display_mode": { "type": "integer", "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" }
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
                                                "line_width": { "type": "integer", "description": "Border width. Default: 1" },
                                                "line_color": { "type": "integer", "description": "Border BGR colour. Default: 0x000080" },
                                                "fill_color": { "type": "integer", "description": "Fill BGR colour. Default: 0xB0FFFF (Altium light yellow)" },
                                                "line_style": { "type": "integer", "description": "Border line style: 0=Solid, 1=Dashed, 2=Dotted. Default: 0" },
                                                "filled": { "type": "boolean", "description": "Whether filled. Default: true" },
                                                "transparent": { "type": "boolean", "description": "Whether the fill is transparent. Default: false" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" },
                                                "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                                                "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                                                "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                                                "owner_part_display_mode": { "type": "integer", "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" }
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
                                                "line_width": { "type": "integer", "description": "Line width. Default: 1" },
                                                "color": { "type": "integer", "description": "Line BGR colour. Default: 0x000080" },
                                                "line_style": { "type": "integer", "description": "Line style: 0=Solid, 1=Dashed, 2=Dotted. Default: 0" },
                                                "is_not_accessible": { "type": "boolean", "description": "Whether the line is marked not-accessible (Altium tags every line; default true)" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" },
                                                "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                                                "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                                                "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                                                "owner_part_display_mode": { "type": "integer", "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" }
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
                                                "line_width": { "type": "integer", "description": "Line width. Default: 1" },
                                                "color": { "type": "integer", "description": "Line BGR colour. Default: 0x000080" },
                                                "line_style": { "type": "integer", "description": "Line style: 0=Solid, 1=Dashed, 2=Dotted. Default: 0" },
                                                "start_line_shape": { "type": "integer", "description": "Start endpoint (arrowhead) shape id. Default: 0 (none)" },
                                                "end_line_shape": { "type": "integer", "description": "End endpoint (arrowhead) shape id. Default: 0 (none)" },
                                                "line_shape_size": { "type": "integer", "description": "Size of the endpoint shapes. Default: 0" },
                                                "transparent": { "type": "boolean", "description": "Whether the polyline is transparent. Default: false" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" },
                                                "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                                                "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                                                "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                                                "owner_part_display_mode": { "type": "integer", "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" }
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
                                                "line_width": { "type": "integer", "description": "Border width. Default: 1" },
                                                "line_color": { "type": "integer", "description": "Border BGR colour. Default: 0x000080" },
                                                "fill_color": { "type": "integer", "description": "Fill BGR colour. Default: 0xB0FFFF (Altium light yellow)" },
                                                "line_style": { "type": "integer", "description": "Border style: 0=Solid, 1=Dashed, 2=Dotted. Default: 0" },
                                                "filled": { "type": "boolean", "description": "Whether filled. Default: true" },
                                                "transparent": { "type": "boolean", "description": "Whether the fill is transparent (vs opaque). Default: false" },
                                                "is_not_accessible": { "type": "boolean", "description": "Whether the polygon is marked not-accessible (Altium tags every polygon; default true)" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" },
                                                "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                                                "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                                                "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                                                "owner_part_display_mode": { "type": "integer", "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" }
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
                                                "line_width": { "type": "integer", "description": "Line width. Default: 1" },
                                                "color": { "type": "integer", "description": "Line BGR colour. Default: 0x000080" },
                                                "fill_color": { "type": "integer", "description": "Fill BGR colour (maps to AreaColor). Default: 0 (no fill)" },
                                                "is_not_accessible": { "type": "boolean", "description": "Whether the arc is marked not-accessible (Altium tags every arc; default true)" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" },
                                                "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                                                "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                                                "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                                                "owner_part_display_mode": { "type": "integer", "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" }
                                            },
                                            "required": ["x", "y", "radius"]
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
                                                "line_width": { "type": "integer", "description": "Border width. Default: 1" },
                                                "line_color": { "type": "integer", "description": "Border BGR colour. Default: 0x000080" },
                                                "fill_color": { "type": "integer", "description": "Fill BGR colour. Default: 0xB0FFFF (Altium light yellow)" },
                                                "filled": { "type": "boolean", "description": "Whether filled. Default: true" },
                                                "transparent": { "type": "boolean", "description": "Whether the fill is transparent. Default: false" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" },
                                                "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                                                "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                                                "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                                                "owner_part_display_mode": { "type": "integer", "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" }
                                            },
                                            "required": ["x", "y", "radius_x", "radius_y"]
                                        }
                                    },
                                    "labels": {
                                        "type": "array",
                                        "description": "Text label definitions (RECORD=4). For RECORD=3 annotations use 'text'.",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "x": { "type": "number", "description": "X position" },
                                                "y": { "type": "number", "description": "Y position" },
                                                "text": { "type": "string", "description": "Text content" },
                                                "font_id": { "type": "integer", "description": "Font ID. Default: 1" },
                                                "color": { "type": "integer", "description": "BGR colour. Default: 0x000080" },
                                                "justification": { "type": "string", "enum": ["bottom_left", "bottom_center", "bottom_right", "middle_left", "middle_center", "middle_right", "top_left", "top_center", "top_right"], "description": "Alignment. Default: bottom_left" },
                                                "rotation": { "type": "number", "description": "Rotation in degrees. Default: 0" },
                                                "is_mirrored": { "type": "boolean", "description": "Mirrored. Default: false" },
                                                "is_hidden": { "type": "boolean", "description": "Hidden. Default: false" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" },
                                                "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                                                "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                                                "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                                                "owner_part_display_mode": { "type": "integer", "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" }
                                            },
                                            "required": ["x", "y", "text"]
                                        }
                                    },
                                    "text": {
                                        "type": "array",
                                        "description": "Text/label annotations",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "x": { "type": "number", "description": "X position" },
                                                "y": { "type": "number", "description": "Y position" },
                                                "text": { "type": "string", "description": "Text content" },
                                                "font_id": { "type": "integer", "description": "Font ID. Default: 1" },
                                                "color": { "type": "integer", "description": "BGR colour. Default: 0x000080" },
                                                "justification": { "type": "string", "enum": ["bottom_left", "bottom_center", "bottom_right", "middle_left", "middle_center", "middle_right", "top_left", "top_center", "top_right"], "description": "Alignment. Default: bottom_left" },
                                                "rotation": { "type": "number", "description": "Rotation in degrees. Default: 0" },
                                                "is_mirrored": { "type": "boolean", "description": "Mirrored. Default: false" },
                                                "is_hidden": { "type": "boolean", "description": "Hidden. Default: false" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID; preserved on read-modify-write, auto-generated if omitted" }
                                            },
                                            "required": ["x", "y", "text"]
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
                                                "font_id": { "type": "integer", "description": "Font ID. Default: 1" },
                                                "color": { "type": "integer", "description": "BGR colour. Default: 0x800000 (dark red)" },
                                                "hidden": { "type": "boolean", "description": "Whether hidden. Default: false" },
                                                "read_only_state": { "type": "integer", "description": "Read-only state (0=editable, 1=read-only). Default: 0" },
                                                "param_type": { "type": "integer", "description": "Parameter type (0=String, 1=Boolean, 2=Integer, 3=Float). Default: 0" },
                                                "unique_id": { "type": "string", "description": "8-char Altium unique ID. Default: auto-generated" },
                                                "orientation": { "type": "integer", "description": "Text orientation (0/1/2/3 = 0/90/180/270 degrees). Default: 0" },
                                                "show_name": { "type": "boolean", "description": "Whether the parameter name is shown alongside the value. Default: false" },
                                                "hide_name": { "type": "boolean", "description": "Whether the parameter name is hidden (only the value shown). Default: false" },
                                                "description": { "type": "string", "description": "Parameter description text. Default: empty" },
                                                "is_configurable": { "type": "boolean", "description": "Whether the parameter is variant-configurable. Default: false" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" },
                                                "graphically_locked": { "type": "boolean", "description": "Whether the shape is graphically locked. Default: false" },
                                                "disabled": { "type": "boolean", "description": "Whether the shape is disabled. Default: false" },
                                                "dimmed": { "type": "boolean", "description": "Whether the shape is dimmed. Default: false" },
                                                "owner_part_display_mode": { "type": "integer", "description": "Display mode this shape belongs to (0=Normal, 1=first alternate/de-Morgan, ...). Default: 0" }
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
                                                "library_path": { "type": "string", "description": "Optional absolute path to the .PcbLib containing the footprint, written as ModelDatafile0 so Altium resolves/previews the model. Omit to link by name only (requires the library to be installed/in the project)." }
                                            },
                                            "required": ["name"]
                                        }
                                    }
                                },
                                "required": ["name", "pins"]
                            }
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
            // === Library Management ===
            ToolDefinition {
                name: "delete_component".to_string(),
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
                example: Some(serde_json::json!({"name": "validate_library", "arguments": {"filepath": "./MyLibrary.PcbLib"}})),
                description: Some(
                    "Validate an Altium library file for common issues. Checks for: empty components \
                     (no pads/pins), duplicate designators, invalid coordinates, zero-size primitives, \
                     and other integrity problems. Returns a list of warnings and errors."
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
                example: Some(serde_json::json!({"name": "export_library", "arguments": {"filepath": "./MyLibrary.PcbLib", "format": "json", "compact": true}})),
                description: Some(
                    "Export an Altium library to JSON or CSV format for version control, backup, \
                     or external processing. JSON includes full component data; CSV provides a \
                     summary table of component names and basic info."
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
                     in the format produced by export_library, enabling round-trip workflows. \
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
                            "description": "For single extraction: file path for .step file. For extract_all: directory path for all models."
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
                            "description": "Maximum number of models to list (for 'list' mode)"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Number of models to skip when listing (for 'list' mode)"
                        }
                    },
                    "required": ["filepath"]
                }),
            },
            ToolDefinition {
                name: "diff_libraries".to_string(),
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
                example: Some(serde_json::json!({"name": "batch_update", "arguments": {"filepath": "./MyLibrary.PcbLib", "operation": "update_track_width", "parameters": {"from_width": 0.2, "to_width": 0.25, "tolerance": 0.001}}})),
                description: Some(
                    "Perform batch updates across all components in an Altium library file. \
                     For PcbLib: update track widths, rename layers. \
                     For SchLib: update parameter values across symbols. \
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
                                    "description": "For update_parameters: parameter name to update (e.g., 'Value')"
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
                example: Some(serde_json::json!({"name": "copy_component", "arguments": {"filepath": "./MyLibrary.PcbLib", "source_name": "RESC0603_IPC_MEDIUM", "target_name": "RESC0603_IPC_MEDIUM_V2", "description": "0603 resistor variant 2"}})),
                description: Some(
                    "Copy/duplicate a component within an Altium library file. Creates a new component \
                     with a different name but identical primitives. Useful for creating variants."
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
                example: Some(serde_json::json!({"name": "copy_component_cross_library", "arguments": {"source_filepath": "./SourceLibrary.PcbLib", "target_filepath": "./TargetLibrary.PcbLib", "component_name": "RESC0603_IPC_MEDIUM", "new_name": "RESC0603_COPIED", "description": "Copied from SourceLibrary", "ignore_missing_models": false, "preserve_external_paths": false}})),
                description: Some(
                    "Copy a component from one Altium library to another. Both libraries must be \
                     the same type (PcbLib to PcbLib, or SchLib to SchLib). Useful for consolidating \
                     libraries or sharing components between projects."
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
                example: Some(serde_json::json!({"name": "merge_libraries", "arguments": {"source_filepaths": ["./LibraryA.PcbLib", "./LibraryB.PcbLib", "./LibraryC.PcbLib"], "target_filepath": "./MergedLibrary.PcbLib", "on_duplicate": "skip"}})),
                description: Some(
                    "Merge multiple Altium libraries into a single library. All source libraries must \
                     be the same type (all PcbLib or all SchLib). Components are copied from each \
                     source into the target library. Use dry_run=true to preview what would be merged."
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
                        "footprint": {
                            "type": "object",
                            "description": "For PcbLib: footprint data (same format as write_pcblib)"
                        },
                        "symbol": {
                            "type": "object",
                            "description": "For SchLib: symbol data (same format as write_schlib)"
                        },
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
                example: Some(serde_json::json!({"name": "render_footprint", "arguments": {"filepath": "./MyLibrary.PcbLib", "component_name": "RESC0603_IPC_MEDIUM", "scale": 2.0, "max_width": 80, "max_height": 40}})),
                description: Some(
                    "Render an ASCII art visualisation of a footprint from a PcbLib file. Shows pads, \
                     tracks, and other primitives in a simple text format for quick preview."
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
                            "description": "Maximum width in characters (default: 80)"
                        },
                        "max_height": {
                            "type": "integer",
                            "description": "Maximum height in characters (default: 40)"
                        }
                    },
                    "required": ["filepath", "component_name"]
                }),
            },
            ToolDefinition {
                name: "render_symbol".to_string(),
                example: Some(serde_json::json!({"name": "render_symbol", "arguments": {"filepath": "./MyLibrary.SchLib", "component_name": "LM358", "scale": 1.0, "max_width": 80, "max_height": 40, "part_id": 1}})),
                description: Some(
                    "Render an ASCII art visualisation of a schematic symbol from a SchLib file. \
                     Shows pins, rectangles, lines, and other primitives in a simple text format \
                     for quick preview. Coordinates are in schematic units (10 units = 1 grid)."
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
                            "description": "Maximum width in characters (default: 80)"
                        },
                        "max_height": {
                            "type": "integer",
                            "description": "Maximum height in characters (default: 40)"
                        },
                        "part_id": {
                            "type": "integer",
                            "description": "Part ID for multi-part symbols (default: 1, shows all parts if 0)"
                        }
                    },
                    "required": ["filepath", "component_name"]
                }),
            },
            // manage_schlib_parameters - Manage symbol parameters (Value, Manufacturer, etc.)
            ToolDefinition {
                name: "manage_schlib_parameters".to_string(),
                example: Some(serde_json::json!({"name": "manage_schlib_parameters", "arguments": {"filepath": "./MyLibrary.SchLib", "component_name": "LM358", "operation": "set", "parameter_name": "Value", "value": "LM358D"}})),
                description: Some(
                    "Manage component parameters in Altium SchLib files. Supports listing, \
                     getting, setting, adding, and deleting parameters like Value, Manufacturer, \
                     Part Number, etc."
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
                            "description": "Name of the parameter (required for get, set, add, delete)"
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
                            "description": "Read-only state (0=editable, 1=read-only) (optional for set, add). Default: 0"
                        },
                        "param_type": {
                            "type": "integer",
                            "description": "Parameter type (0=String, 1=Boolean, 2=Integer, 3=Float) (optional for set, add). Default: 0"
                        },
                        "unique_id": {
                            "type": "string",
                            "description": "8-char Altium unique ID (optional for set, add). Default: auto-generated"
                        },
                        "x": {
                            "type": "integer",
                            "description": "X position in schematic units (optional for add)"
                        },
                        "y": {
                            "type": "integer",
                            "description": "Y position in schematic units (optional for add)"
                        }
                    },
                    "required": ["filepath", "component_name", "operation"]
                }),
            },
            // manage_schlib_footprints - Manage footprint links in symbols
            ToolDefinition {
                name: "manage_schlib_footprints".to_string(),
                example: Some(serde_json::json!({"name": "manage_schlib_footprints", "arguments": {"filepath": "./MyLibrary.SchLib", "component_name": "LM358", "operation": "add", "footprint_name": "SOIC-8_3.9x4.9mm"}})),
                description: Some(
                    "Manage footprint links in Altium SchLib symbols. Supports listing, adding, \
                     and removing footprint references that link schematic symbols to PCB footprints."
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
                            "description": "Footprint description (optional for add)"
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
                example: Some(serde_json::json!({"name": "compare_components", "arguments": {"filepath_a": "./LibraryA.PcbLib", "component_a": "RESC0603_V1", "filepath_b": "./LibraryB.PcbLib", "component_b": "RESC0603_V2", "include_geometry": true, "tolerance": 0.001}})),
                description: Some(
                    "Compare two specific components in detail, showing differences in primitives, \
                     parameters, and properties. Components can be from the same library or different \
                     libraries. Returns detailed primitive-level differences (pads, tracks, pins, etc.)."
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
                example: Some(serde_json::json!({"name": "repair_library", "arguments": {"filepath": "./MyLibrary.PcbLib", "dry_run": true}})),
                description: Some(
                    "Repair a library by removing orphaned references. For PcbLib files, this removes: \
                     (1) embedded models not referenced by any footprint, and \
                     (2) component body references that point to non-existent models. \
                     This fixes libraries where STEP model data is missing but references remain."
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
                example: Some(serde_json::json!({"name": "restore_backup", "arguments": {"filepath": "./MyLibrary.PcbLib", "backup_path": "MyLibrary.PcbLib.20260125_091500.bak"}})),
                description: Some(
                    "Restore an Altium library file from a backup. If no specific backup is specified, \
                     restores from the most recent backup."
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
                example: Some(serde_json::json!({"name": "update_pad", "arguments": {"filepath": "./MyLibrary.PcbLib", "component_name": "RESC0603", "designator": "1", "updates": {"width": 1.0, "height": 0.9, "shape": "rectangle"}, "dry_run": false}})),
                description: Some(
                    "Update specific properties of a pad in a PcbLib footprint without replacing \
                     the entire component. Find pad by designator and apply only the specified updates."
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
                                "shape": { "type": "string", "description": "New shape (Rectangle, Round, Oval, Octagonal, RoundedRectangle)" },
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
                example: Some(serde_json::json!({"name": "update_primitive", "arguments": {"filepath": "./MyLibrary.PcbLib", "component_name": "RESC0603", "primitive_type": "track", "index": 0, "updates": {"width": 0.15, "layer": "Top Overlay"}, "dry_run": false}})),
                description: Some(
                    "Update specific properties of a primitive (track, arc, region, or text) in a \
                     PcbLib footprint. Find primitive by type and index, apply only specified updates."
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
                            "enum": ["track", "arc", "region", "text", "fill"],
                            "description": "Type of primitive to update"
                        },
                        "index": {
                            "type": "integer",
                            "description": "Zero-based index of the primitive within its type array"
                        },
                        "updates": {
                            "type": "object",
                            "description": "Properties to update (only specified properties are changed). Valid properties depend on primitive_type.",
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
                                "rotation": { "type": "number", "description": "Rotation angle" },
                                "layer": { "type": "string", "description": "Layer name" }
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
