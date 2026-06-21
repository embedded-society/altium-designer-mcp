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
                description: Some(
                    "Write footprints to an Altium .PcbLib file. Each footprint is defined by \
                     its primitives: pads (with position, size, shape, layer), tracks, arcs, \
                     regions, and text. The AI is responsible for calculating correct positions \
                     and sizes based on IPC-7351B or other standards. \
                     All coordinates and dimensions must be in millimetres (mm)."
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
                                                    "enum": ["rectangle", "rounded_rectangle", "round", "circle", "oval"],
                                                    "description": "Pad shape: rectangle (pin 1), rounded_rectangle (SMD default), round/circle (equivalent, for through-hole), oval"
                                                },
                                                "layer": { "type": "string", "description": "Layer name: Top Layer, Bottom Layer, Multi-Layer (default for SMD)" },
                                                "hole_size": { "type": "number", "description": "Hole diameter for through-hole pads (mm)" }
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
                                                "layer": { "type": "string", "description": "Layer name: Top Overlay, Top Assembly, Top Courtyard, Mechanical 1, etc." }
                                            },
                                            "required": ["x1", "y1", "x2", "y2", "width", "layer"]
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
                                                "layer": { "type": "string", "description": "Layer name: Top Overlay, Top Assembly, Mechanical 1, etc." }
                                            },
                                            "required": ["x", "y", "radius", "start_angle", "end_angle", "width", "layer"]
                                        }
                                    },
                                    "regions": {
                                        "type": "array",
                                        "description": "Filled region definitions (courtyard, etc.)",
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
                                                "layer": { "type": "string", "description": "Layer name: Top Courtyard, Top Assembly, Mechanical 1, etc." }
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
                                                "rotation": { "type": "number", "description": "Rotation in degrees" }
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
                                    }
                                },
                                "required": ["name", "pads"]
                            }
                        },
                        "append": {
                            "type": "boolean",
                            "description": "If true, append to existing file; if false, create new file"
                        }
                    },
                    "required": ["filepath", "footprints"]
                }),
            },
            ToolDefinition {
                name: "write_schlib".to_string(),
                description: Some(
                    "Write schematic symbols to an Altium .SchLib file. Each symbol is defined by \
                     its primitives: pins, rectangles, lines, polylines, arcs, ellipses, and labels. \
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
                                    "designator_prefix": { "type": "string", "description": "e.g., 'R' for resistors, 'U' for ICs" },
                                    "part_count": { "type": "integer", "description": "Number of parts for multi-part symbols (e.g., 2 for dual op-amp). Default: 1" },
                                    "pins": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "designator": { "type": "string" },
                                                "name": { "type": "string" },
                                                "x": { "type": "number" },
                                                "y": { "type": "number" },
                                                "length": { "type": "number" },
                                                "orientation": { "type": "string", "enum": ["left", "right", "up", "down"] },
                                                "electrical_type": { "type": "string", "enum": ["input", "output", "bidirectional", "passive", "power"] },
                                                "owner_part_id": { "type": "integer", "description": "Part number this pin belongs to (1-based). Default: 1" }
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
                                                "x1": { "type": "integer", "description": "Left X coordinate" },
                                                "y1": { "type": "integer", "description": "Bottom Y coordinate" },
                                                "x2": { "type": "integer", "description": "Right X coordinate" },
                                                "y2": { "type": "integer", "description": "Top Y coordinate" },
                                                "line_width": { "type": "integer", "description": "Border width. Default: 1" },
                                                "line_color": { "type": "integer", "description": "Border BGR colour. Default: 0x000080" },
                                                "fill_color": { "type": "integer", "description": "Fill BGR colour. Default: 0xFFFFB0" },
                                                "filled": { "type": "boolean", "description": "Whether filled. Default: true" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" }
                                            },
                                            "required": ["x1", "y1", "x2", "y2"]
                                        }
                                    },
                                    "lines": {
                                        "type": "array",
                                        "description": "Line definitions",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "x1": { "type": "integer", "description": "Start X coordinate" },
                                                "y1": { "type": "integer", "description": "Start Y coordinate" },
                                                "x2": { "type": "integer", "description": "End X coordinate" },
                                                "y2": { "type": "integer", "description": "End Y coordinate" },
                                                "line_width": { "type": "integer", "description": "Line width. Default: 1" },
                                                "color": { "type": "integer", "description": "Line BGR colour. Default: 0x000080" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" }
                                            },
                                            "required": ["x1", "y1", "x2", "y2"]
                                        }
                                    },
                                    "text": {
                                        "type": "array",
                                        "description": "Text/label annotations",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "x": { "type": "integer", "description": "X position" },
                                                "y": { "type": "integer", "description": "Y position" },
                                                "text": { "type": "string", "description": "Text content" },
                                                "font_id": { "type": "integer", "description": "Font ID. Default: 1" },
                                                "color": { "type": "integer", "description": "BGR colour. Default: 0x000080" },
                                                "justification": { "type": "string", "enum": ["bottom_left", "bottom_center", "bottom_right", "middle_left", "middle_center", "middle_right", "top_left", "top_center", "top_right"], "description": "Alignment. Default: bottom_left" },
                                                "rotation": { "type": "number", "description": "Rotation in degrees. Default: 0" },
                                                "is_mirrored": { "type": "boolean", "description": "Mirrored. Default: false" },
                                                "is_hidden": { "type": "boolean", "description": "Hidden. Default: false" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" }
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
                                                "x": { "type": "integer", "description": "X position. Default: 0" },
                                                "y": { "type": "integer", "description": "Y position. Default: 0" },
                                                "font_id": { "type": "integer", "description": "Font ID. Default: 1" },
                                                "color": { "type": "integer", "description": "BGR colour. Default: 0x800000 (dark red)" },
                                                "hidden": { "type": "boolean", "description": "Whether hidden. Default: false" },
                                                "owner_part_id": { "type": "integer", "description": "Part number (1-based). Default: 1" }
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
            // === Library Management ===
            ToolDefinition {
                name: "delete_component".to_string(),
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
                        }
                    },
                    "required": ["filepath", "component_name", "operation"]
                }),
            },
            ToolDefinition {
                name: "compare_components".to_string(),
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
                                "shape": { "type": "string", "description": "New shape (Rectangle, Round, Oval, RoundedRectangle)" },
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
