# altium-designer-mcp

**An AI-operated Altium Designer libraries editor.**

An MCP server that provides file I/O and primitive placement tools, enabling AI assistants
(Claude Code, Claude Desktop, VSCode Copilot) to create and manage Altium Designer
component libraries.

---

## The Core Idea

**The AI handles the intelligence. The tool handles file I/O.**

| Responsibility | Owner |
|---------------|-------|
| IPC-7351B calculations | AI |
| Package layout decisions | AI |
| Style choices | AI |
| Datasheet interpretation | AI |
| Reading/writing Altium files | This tool |
| Primitive placement | This tool |
| STEP model attachment | This tool |

This means the AI can create **any footprint** — not just pre-programmed package types.
See [docs/VISION.md](docs/VISION.md) for the full architectural rationale.

---

## Quick Start with Claude Code

> **[Claude Code Setup Guide](docs/CLAUDE_CODE_GUIDE.md)** — Complete step-by-step instructions
> for using this MCP server with Claude Code CLI on **Windows**, **Linux**, and **macOS**.

---

## How It Works

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│  AI-ASSISTED COMPONENT CREATION                                             │
│                                                                             │
│  Engineer                    AI                         MCP Server          │
│    │                         │                              │               │
│    │  "Create 0603 resistor" │                              │               │
│    ├────────────────────────►│                              │               │
│    │                         │                              │               │
│    │                         │  AI reasons about:           │               │
│    │                         │  • IPC-7351B pad sizes       │               │
│    │                         │  • Courtyard margins         │               │
│    │                         │  • Silkscreen/symbol style   │               │
│    │                         │                              │               │
│    │                         │  write_pcblib(primitives)    │               │
│    │                         ├─────────────────────────────►│               │
│    │                         │                              │ Writes        │
│    │                         │                              │ .PcbLib +     │
│    │                         │  write_schlib(symbol)        │ .SchLib files │
│    │                         ├─────────────────────────────►│               │
│    │                         │◄─────────────────────────────┤               │
│    │                         │  { success: true }           │               │
│    │                         │                              │               │
│    │  "Done! Footprint       │                              │               │
│    │   and symbol created"   │                              │               │
│    │◄────────────────────────┤                              │               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## MCP Tools

### `read_pcblib`

Read footprints from an Altium `.PcbLib` file. All coordinates are in millimetres.

```json
{
    "name": "read_pcblib",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib"
    }
}
```

**Pagination options** (for large libraries):

```json
{
    "name": "read_pcblib",
    "arguments": {
        "filepath": "./LargeLibrary.PcbLib",
        "component_name": "RESC1608X55N",
        "limit": 10,
        "offset": 0,
        "compact": true
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `component_name` | Fetch only this specific footprint |
| `limit` | Maximum footprints to return |
| `offset` | Skip first N footprints |
| `compact` | Omit redundant per-layer pad data when uniform (default: `true`) |

### `write_pcblib`

Write footprints to an Altium `.PcbLib` file. The AI provides primitive definitions.

```json
{
    "name": "write_pcblib",
    "arguments": {
        "filepath": "./Passives.PcbLib",
        "footprints": [{
            "name": "RESC1608X55N",
            "description": "Chip resistor, 0603 (1608 metric)",
            "pads": [
                { "designator": "1", "x": -0.75, "y": 0, "width": 0.9, "height": 0.95 },
                { "designator": "2", "x": 0.75, "y": 0, "width": 0.9, "height": 0.95 }
            ],
            "tracks": [
                { "x1": -0.8, "y1": -0.425, "x2": 0.8, "y2": -0.425, "width": 0.12, "layer": "Top Overlay" },
                { "x1": -0.8, "y1": 0.425, "x2": 0.8, "y2": 0.425, "width": 0.12, "layer": "Top Overlay" }
            ],
            "regions": [
                { "vertices": [{"x": -1.45, "y": -0.73}, {"x": 1.45, "y": -0.73}, {"x": 1.45, "y": 0.73}, {"x": -1.45, "y": 0.73}], "layer": "Top Courtyard" }
            ]
        }],
        "append": false
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `append` | If `true`, add footprints to existing file; if `false`, create new file (default: `false`) |

### `read_schlib`

Read symbols from an Altium `.SchLib` file. Coordinates are in schematic units (10 units = 1 grid).

```json
{
    "name": "read_schlib",
    "arguments": {
        "filepath": "./MySymbols.SchLib"
    }
}
```

**Pagination options** (for large libraries):

```json
{
    "name": "read_schlib",
    "arguments": {
        "filepath": "./LargeLibrary.SchLib",
        "component_name": "RES_0603",
        "limit": 10,
        "offset": 0
    }
}
```

### `write_schlib`

Write symbols to an Altium `.SchLib` file. The AI provides primitive definitions.

```json
{
    "name": "write_schlib",
    "arguments": {
        "filepath": "./MySymbols.SchLib",
        "symbols": [...],
        "append": false
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `append` | If `true`, add symbols to existing file; if `false`, create new file (default: `false`) |

**Symbol properties:**

| Property | Description |
|----------|-------------|
| `name` | Symbol name (required) |
| `description` | Symbol description |
| `designator_prefix` | Designator prefix (e.g., "R", "U", "C") |
| `part_count` | Number of parts for multi-part symbols (default: 1) |
| `pins` | Array of pin definitions |

**Pin properties:**

| Property | Description |
|----------|-------------|
| `designator` | Pin number (required) |
| `name` | Pin name (required) |
| `x`, `y` | Position in schematic units (required) |
| `length` | Pin length (required) |
| `orientation` | `left`, `right`, `up`, `down` (required) |
| `electrical_type` | `input`, `output`, `bidirectional`, `passive`, `power` |
| `owner_part_id` | Part number for multi-part symbols (1-based, default: 1) |
| `symbol_inner_edge` | Pin symbol at inner edge: `none`, `dot`, `clock`, `schmitt`, etc. |
| `symbol_outer_edge` | Pin symbol at outer edge: `none`, `dot`, `active_low_input`, etc. |
| `symbol_inside` | Pin symbol inside: `none`, `dot`, `clock`, etc. |
| `symbol_outside` | Pin symbol outside: `none`, `dot`, `clock`, etc. |

### `list_components`

List component names in an Altium library file. Supports pagination for large libraries.

```json
{
    "name": "list_components",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "limit": 50,
        "offset": 0,
        "include_metadata": true
    }
}
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `filepath` | Yes | Path to the library file |
| `limit` | No | Maximum number of components to return (default: all) |
| `offset` | No | Number of components to skip (default: 0) |
| `include_metadata` | No | Include component metadata like pad/pin counts (default: `false`) |

**Response includes:**

- `total_count`: Total number of components in the library
- `returned_count`: Number of components in this response
- `offset`: Current offset
- `has_more`: Whether more components are available

**With `include_metadata: true` (PcbLib):**

```json
{
    "components": [
        { "name": "RESC0603", "pad_count": 2, "track_count": 4, "has_3d_model": true }
    ]
}
```

**With `include_metadata: true` (SchLib):**

```json
{
    "components": [
        { "name": "RESISTOR", "part_count": 1, "pin_count": 2, "footprint_count": 1 }
    ]
}
```

### `extract_style`

Extract styling information from an existing library.

```json
{
    "name": "extract_style",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib"
    }
}
```

Returns statistics about track widths, pad shapes, pin lengths, colours, and layer usage.

### `delete_component`

Delete one or more components from an Altium library file. Works with both `.PcbLib` and `.SchLib` files.
Use `dry_run=true` to preview changes without modifying the file.

```json
{
    "name": "delete_component",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "component_names": ["OLD_FOOTPRINT", "UNUSED_COMPONENT"],
        "dry_run": false
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `component_names` | Array of component names to delete |
| `dry_run` | If `true`, show what would be deleted without modifying the file (default: `false`) |

Returns per-component status (`deleted` or `not_found`) and updated component counts.

A backup is automatically created before deletion (see [Automatic Backups](#automatic-backups)).

### `validate_library`

Validate an Altium library file for common issues. Works with both `.PcbLib` and `.SchLib` files.

```json
{
    "name": "validate_library",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib"
    }
}
```

Checks for:

- Empty components (no pads/pins)
- Duplicate designators within a component
- Invalid coordinates (NaN, Infinity)
- Zero or negative dimensions
- Missing body graphics (SchLib)

Returns status (`valid`, `warnings`, or `invalid`) with a list of issues found.

### `export_library`

Export an Altium library to JSON or CSV format for version control, backup, or external processing.

```json
{
    "name": "export_library",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "format": "json",
        "compact": true
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `format` | Export format: `json` for full data, `csv` for summary table |
| `compact` | Omit redundant per-layer pad data when uniform (default: `true`) |

**JSON format** returns complete component data including all primitives.

**CSV format** returns a summary table with columns: name, description, pad/pin count, etc.

### `extract_step_model`

Extract embedded STEP 3D models from an Altium .PcbLib file. Models are stored compressed inside the library;
this tool extracts them to standalone .step files. Supports multiple extraction modes and pagination.

```json
{
    "name": "extract_step_model",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "output_path": "./extracted_model.step",
        "model": "RESC1005X04L.step",
        "mode": "auto"
    }
}
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `filepath` | Yes | Path to the .PcbLib file containing embedded 3D models |
| `output_path` | No | Path where the extracted .step file will be saved. If omitted, returns base64-encoded data. |
| `model` | No | Model name (e.g., `RESC1005X04L.step`) or GUID to extract. If omitted and only one model exists, extracts it automatically. If multiple models exist and no model specified, lists available models. |
| `mode` | No | Extraction mode: `auto` (default), `list`, `extract_all`, `extract_by_footprint` |
| `footprint_name` | No | Footprint name (required for `extract_by_footprint` mode) |
| `limit` | No | Maximum number of models to list (default: all) |
| `offset` | No | Number of models to skip when listing (default: 0) |

**Extraction Modes:**

| Mode | Description |
|------|-------------|
| `auto` | Default behaviour — extract single model, or list if multiple exist |
| `list` | List all available models without extracting |
| `extract_all` | Extract all models to a directory (requires `output_path` to be a directory) |
| `extract_by_footprint` | Extract models used by a specific footprint (requires `footprint_name`) |

**Response (listing models):**

```json
{
    "status": "list",
    "filepath": "./MyLibrary.PcbLib",
    "message": "Multiple models found. Specify 'model' parameter with name or ID to extract.",
    "total_count": 50,
    "returned_count": 10,
    "offset": 0,
    "has_more": true,
    "models": [
        { "id": "{GUID...}", "name": "model1.step", "size_bytes": 12345 },
        { "id": "{GUID...}", "name": "model2.step", "size_bytes": 67890 }
    ]
}
```

**Response (extraction success):**

```json
{
    "status": "success",
    "filepath": "./MyLibrary.PcbLib",
    "output_path": "./extracted_model.step",
    "model_id": "{GUID...}",
    "model_name": "RESC1005X04L.step",
    "size_bytes": 12345,
    "message": "STEP model extracted to './extracted_model.step'"
}
```

### `import_library`

Import components into an Altium library from JSON data. This is the inverse of `export_library` —
it accepts the same JSON format that `export_library` produces, enabling round-trip workflows.

```json
{
    "name": "import_library",
    "arguments": {
        "output_path": "./NewLibrary.PcbLib",
        "json_data": {
            "file_type": "PcbLib",
            "footprints": [...]
        },
        "append": false
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `output_path` | Path for the output library file (.PcbLib or .SchLib) |
| `json_data` | JSON object containing the library data (same format as `export_library` output) |
| `append` | If `true`, add components to existing file; if `false`, create new file (default: `false`) |

**JSON format (PcbLib):**

```json
{
    "file_type": "PcbLib",
    "footprints": [
        {
            "name": "RESC1608X55N",
            "description": "Chip resistor, 0603",
            "pads": [...],
            "tracks": [...],
            "regions": [...]
        }
    ]
}
```

**JSON format (SchLib):**

```json
{
    "file_type": "SchLib",
    "symbols": [
        {
            "name": "RESISTOR",
            "description": "Generic resistor",
            "pins": [...]
        }
    ]
}
```

The library type is determined from:

1. The `file_type` field in the JSON data (preferred)
2. The output file extension (.PcbLib or .SchLib)

**Use cases:**

- Round-trip editing: export → modify JSON → import
- Version control: store libraries as JSON, import when needed
- Migration: convert between formats or merge data from external sources
- Backup restoration: recreate libraries from JSON backups

### `diff_libraries`

Compare two Altium library files and report differences. Both files must be the same type.

```json
{
    "name": "diff_libraries",
    "arguments": {
        "filepath_a": "./OldLibrary.PcbLib",
        "filepath_b": "./NewLibrary.PcbLib"
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath_a` | Path to the first (base/old) library |
| `filepath_b` | Path to the second (new/changed) library |

Returns:

- **added**: Components in B but not in A
- **removed**: Components in A but not in B
- **modified**: Components in both with changes (count differences, description changes)

### `batch_update`

Perform batch updates across all components in an Altium library file. Supports PcbLib
operations (track width updates, layer renames) and SchLib operations (parameter updates).

```json
{
    "name": "batch_update",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "operation": "update_track_width",
        "parameters": {
            "from_width": 0.2,
            "to_width": 0.25,
            "tolerance": 0.001
        }
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath` | Path to the PcbLib or SchLib file |
| `operation` | One of: `update_track_width`, `rename_layer`, `update_parameters` |
| `parameters` | Operation-specific parameters |

**PcbLib Operations:**

| Operation | Parameters | Description |
|-----------|------------|-------------|
| `update_track_width` | `from_width`, `to_width`, `tolerance` | Update all tracks matching `from_width` (±tolerance) to `to_width` |
| `rename_layer` | `from_layer`, `to_layer` | Change all primitives from one layer to another |

**SchLib Operations:**

| Operation | Parameters | Description |
|-----------|------------|-------------|
| `update_parameters` | `param_name`, `param_value`, `symbol_filter`?, `add_if_missing`? | Update parameter values across symbols |

**Example: Rename layer (PcbLib)**

```json
{
    "name": "batch_update",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "operation": "rename_layer",
        "parameters": {
            "from_layer": "Mechanical 1",
            "to_layer": "Mechanical 2"
        }
    }
}
```

Layer names accept both spaced format (`Top Layer`) and camelCase (`TopLayer`).

**Example: Update parameters across symbols (SchLib)**

```json
{
    "name": "batch_update",
    "arguments": {
        "filepath": "./MySymbols.SchLib",
        "operation": "update_parameters",
        "parameters": {
            "param_name": "Manufacturer",
            "param_value": "Acme Corp",
            "symbol_filter": "^RES.*",
            "add_if_missing": true
        }
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `param_name` | Name of the parameter to update |
| `param_value` | New value for the parameter |
| `symbol_filter` | Optional regex to filter which symbols to update |
| `add_if_missing` | If true, add the parameter to symbols that don't have it (default: false) |

### `copy_component`

Copy/duplicate a component within an Altium library file. Creates a new component with a
different name but identical primitives. Useful for creating variants.

```json
{
    "name": "copy_component",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "source_name": "RESC0603_IPC_MEDIUM",
        "target_name": "RESC0603_IPC_MEDIUM_V2",
        "description": "0603 resistor variant 2"
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath` | Path to the library file (.PcbLib or .SchLib) |
| `source_name` | Name of the component to copy |
| `target_name` | Name for the new copied component |
| `description` | Optional description for the new component |

Returns the new component count after copying.

### `rename_component`

Rename a component within an Altium library file. This is an atomic operation that changes
the component's name while preserving all primitives and properties. More efficient than
copy + delete for simple renames.

```json
{
    "name": "rename_component",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "old_name": "RESC0603_OLD",
        "new_name": "RESC0603_NEW"
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath` | Path to the library file (.PcbLib or .SchLib) |
| `old_name` | Current name of the component to rename |
| `new_name` | New name for the component |

Returns the component count after renaming (unchanged).

### `copy_component_cross_library`

Copy a component from one Altium library to another. Both libraries must be the same type
(PcbLib to PcbLib, or SchLib to SchLib). Useful for consolidating libraries or sharing
components between projects.

```json
{
    "name": "copy_component_cross_library",
    "arguments": {
        "source_filepath": "./SourceLibrary.PcbLib",
        "target_filepath": "./TargetLibrary.PcbLib",
        "component_name": "RESC0603_IPC_MEDIUM",
        "new_name": "RESC0603_COPIED",
        "description": "Copied from SourceLibrary",
        "ignore_missing_models": false,
        "preserve_external_paths": false
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `source_filepath` | Path to the source library file (.PcbLib or .SchLib) |
| `target_filepath` | Path to the target library file (must be same type as source) |
| `component_name` | Name of the component to copy from the source library |
| `new_name` | Optional new name for the component in the target library (defaults to original name) |
| `description` | Optional new description for the component (defaults to original description) |
| `ignore_missing_models` | If `true`, copy the component even if referenced embedded 3D models are missing (PcbLib only). The component body references will be removed. (default: `false`) |
| `preserve_external_paths` | If `true`, keep external 3D model file path references (default: `false` — external paths are removed as they are not portable) |

**Behaviour:**

- If the target file does not exist, it will be created
- If the target file exists, the component will be added to it
- If a component with the same name already exists in the target, an error is returned
- Embedded 3D models are copied along with the component (if present and valid)
- External STEP file references are removed by default (paths are not portable); use `preserve_external_paths: true` to keep them

### `merge_libraries`

Merge multiple Altium libraries into a single library. All source libraries must be the same
type (all PcbLib or all SchLib). Components are copied from each source into the target library.

```json
{
    "name": "merge_libraries",
    "arguments": {
        "source_filepaths": [
            "./LibraryA.PcbLib",
            "./LibraryB.PcbLib",
            "./LibraryC.PcbLib"
        ],
        "target_filepath": "./MergedLibrary.PcbLib",
        "on_duplicate": "skip"
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `source_filepaths` | Array of paths to source library files (.PcbLib or .SchLib) |
| `target_filepath` | Path to the target library file (will be created or appended to) |
| `on_duplicate` | How to handle duplicate names: `skip` (ignore), `error` (fail), `rename` (auto-suffix). Default: `error` |

**Behaviour:**

- If the target file does not exist, it will be created
- If the target file exists, components will be appended to it
- Duplicate handling options:
    - `error`: Fail immediately if a duplicate is found (default)
    - `skip`: Silently ignore duplicates, keeping the first occurrence
    - `rename`: Auto-rename duplicates with `_1`, `_2`, etc. suffixes

**Example Response:**

```json
{
    "status": "success",
    "target_filepath": "./MergedLibrary.PcbLib",
    "file_type": "PcbLib",
    "sources_count": 3,
    "merged_count": 45,
    "skipped_count": 2,
    "renamed_count": 0,
    "final_count": 45,
    "message": "Merged 45 components from 3 sources into './MergedLibrary.PcbLib' (total: 45)"
}
```

### `reorder_components`

Reorder components in an Altium library file (.PcbLib or .SchLib). Specify the desired order as
a list of component names. Components not in the list are placed at the end in their original
relative order.

```json
{
    "name": "reorder_components",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "component_order": ["RESC1608X55N", "RESC0805X40N", "RESC0402X20N"]
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath` | Path to the .PcbLib or .SchLib file |
| `component_order` | Component names in desired order |

**Behaviour:**

- Components listed in `component_order` appear first, in the specified order
- Components not in the list are appended at the end in their original relative order
- Names in `component_order` that don't exist in the library are ignored

**Example Response:**

```json
{
    "status": "success",
    "filepath": "./MyLibrary.PcbLib",
    "component_count": 5,
    "original_order": ["CAPC0402X20N", "RESC1608X55N", "RESC0805X40N", "RESC0402X20N", "INDC1005X55N"],
    "new_order": ["RESC1608X55N", "RESC0805X40N", "RESC0402X20N", "CAPC0402X20N", "INDC1005X55N"],
    "not_in_library": [],
    "appended_at_end": ["CAPC0402X20N", "INDC1005X55N"],
    "message": "Reordered 5 components in './MyLibrary.PcbLib' (2 components appended at end)"
}
```

### `update_component`

Update a component in-place within an Altium library file, preserving its position. For PcbLib
files, provide a `footprint` object. For SchLib files, provide a `symbol` object. The component
is matched by the `component_name` parameter.

```json
{
    "name": "update_component",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "component_name": "RESC0402X20N",
        "footprint": {
            "name": "RESC0402X20N",
            "description": "Updated resistor 0402",
            "pads": [
                {"designator": "1", "x": -0.5, "y": 0, "width": 0.5, "height": 0.5, "layer": "TopLayer"},
                {"designator": "2", "x": 0.5, "y": 0, "width": 0.5, "height": 0.5, "layer": "TopLayer"}
            ]
        }
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath` | Path to the .PcbLib or .SchLib file |
| `component_name` | Name of the component to update (must exist) |
| `footprint` | For PcbLib: footprint data (same format as `write_pcblib`) |
| `symbol` | For SchLib: symbol data (same format as `write_schlib`) |

**Example Response:**

```json
{
    "status": "success",
    "filepath": "./MyLibrary.PcbLib",
    "file_type": "PcbLib",
    "component_name": "RESC0402X20N",
    "new_name": "RESC0402X20N",
    "renamed": false,
    "component_count": 5,
    "message": "Updated component 'RESC0402X20N' in './MyLibrary.PcbLib'"
}
```

### `search_components`

Search for components across multiple Altium libraries using regex or glob patterns. Returns
matching component names with their source library paths. Supports both `.PcbLib` (footprints)
and `.SchLib` (symbols) files.

```json
{
    "name": "search_components",
    "arguments": {
        "filepaths": [
            "./Resistors.PcbLib",
            "./Capacitors.PcbLib",
            "./ICs.PcbLib"
        ],
        "pattern": "SOIC-*",
        "pattern_type": "glob"
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepaths` | Array of library file paths to search (.PcbLib or .SchLib) |
| `pattern` | Search pattern to match component names |
| `pattern_type` | Pattern type: `glob` (wildcards like `*` and `?`) or `regex`. Default: `glob` |

**Glob Patterns:**

- `*` matches any number of characters
- `?` matches a single character
- Search is case-insensitive

**Example Response:**

```json
{
    "status": "success",
    "pattern": "SOIC-*",
    "pattern_type": "glob",
    "libraries_searched": 3,
    "components_searched": 150,
    "matches_found": 5,
    "matches": [
        { "name": "SOIC-8", "library": "./ICs.PcbLib", "type": "PcbLib" },
        { "name": "SOIC-14", "library": "./ICs.PcbLib", "type": "PcbLib" },
        { "name": "SOIC-16", "library": "./ICs.PcbLib", "type": "PcbLib" }
    ],
    "message": "Found 5 matches for 'SOIC-*' across 3 libraries (150 components searched)"
}
```

### `get_component`

Get a single component by name from an Altium library. Returns the full component data
(footprint or symbol) without needing to read and filter the entire library. Supports both
`.PcbLib` (footprints) and `.SchLib` (symbols) files.

```json
{
    "name": "get_component",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "component_name": "SOIC-8"
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath` | Path to the Altium library file (.PcbLib or .SchLib) |
| `component_name` | Exact name of the component to retrieve |

**Example Response (PcbLib):**

```json
{
    "status": "success",
    "filepath": "./MyLibrary.PcbLib",
    "component_name": "SOIC-8",
    "type": "PcbLib",
    "units": "mm",
    "component": {
        "name": "SOIC-8",
        "description": "8-pin SOIC package",
        "pads": [...],
        "tracks": [...]
    },
    "message": "Retrieved footprint 'SOIC-8' from './MyLibrary.PcbLib'"
}
```

**Error Response (component not found):**

```json
{
    "isError": true,
    "content": [{
        "type": "text",
        "text": "Component 'SOIC-99' not found in library. Available components: SOIC-8, SOIC-14, SOIC-16 ... and 5 more"
    }]
}
```

### `compare_components`

Compare two specific components in detail, showing differences in primitives, parameters, and
properties. Components can be from the same library or different libraries. Returns detailed
primitive-level differences (pads, tracks, pins, etc.).

```json
{
    "name": "compare_components",
    "arguments": {
        "filepath_a": "./LibraryA.PcbLib",
        "component_a": "RESC0603_V1",
        "filepath_b": "./LibraryB.PcbLib",
        "component_b": "RESC0603_V2",
        "include_geometry": true,
        "tolerance": 0.001
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath_a` | Path to the first library file (.PcbLib or .SchLib) |
| `component_a` | Name of the first component |
| `filepath_b` | Path to the second library file (can be same as `filepath_a`) |
| `component_b` | Name of the second component |
| `include_geometry` | Include detailed geometry comparisons (default: `true`) |
| `tolerance` | Tolerance for floating-point comparisons in mm (default: `0.001`) |

**Example Response:**

```json
{
    "status": "different",
    "summary": {
        "identical": false,
        "pad_differences": 2,
        "track_differences": 1,
        "description_changed": true
    },
    "differences": {
        "pads": [
            { "designator": "1", "field": "width", "a": 0.9, "b": 1.0 }
        ]
    }
}
```

### `render_footprint`

Render an ASCII art visualisation of a footprint from a PcbLib file. Shows pads, tracks,
and other primitives in a simple text format for quick preview.

```json
{
    "name": "render_footprint",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "component_name": "RESC0603_IPC_MEDIUM",
        "scale": 2.0,
        "max_width": 80,
        "max_height": 40
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath` | Path to the PcbLib file |
| `component_name` | Name of the footprint to render |
| `scale` | Characters per mm (default: 2.0) |
| `max_width` | Maximum width in characters (default: 80) |
| `max_height` | Maximum height in characters (default: 40) |

Returns ASCII art with pad designators shown in full (e.g., "1", "10", "A01"). Legend: `#` = pad area, `-` = track, `o` = arc, `+` = origin.

### `render_symbol`

Render an ASCII art visualisation of a schematic symbol from a SchLib file. Shows pins,
rectangles, lines, and other primitives in a simple text format for quick preview.

```json
{
    "name": "render_symbol",
    "arguments": {
        "filepath": "./MyLibrary.SchLib",
        "component_name": "LM358",
        "scale": 1.0,
        "max_width": 80,
        "max_height": 40,
        "part_id": 1
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath` | Path to the SchLib file |
| `component_name` | Name of the symbol to render |
| `scale` | Characters per 10 schematic units (default: 1.0) |
| `max_width` | Maximum width in characters (default: 80) |
| `max_height` | Maximum height in characters (default: 40) |
| `part_id` | Part ID for multi-part symbols (default: 1, use 0 for all parts) |

Returns ASCII art with pin designators shown in full (e.g., "1", "10", "VCC"). Legend: `|-+` = rectangle, `~` = pin line, `o` = arc, `O` = ellipse, `+` = origin.

### `manage_schlib_parameters`

Manage component parameters in Altium SchLib files. Supports listing, getting, setting,
adding, and deleting parameters like Value, Manufacturer, Part Number, etc.

```json
{
    "name": "manage_schlib_parameters",
    "arguments": {
        "filepath": "./MyLibrary.SchLib",
        "component_name": "LM358",
        "operation": "set",
        "parameter_name": "Value",
        "value": "LM358D"
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath` | Path to the SchLib file |
| `component_name` | Name of the symbol |
| `operation` | Operation: `list`, `get`, `set`, `add`, `delete` |
| `parameter_name` | Parameter name (required for get/set/add/delete) |
| `value` | Parameter value (required for set/add) |
| `hidden` | Whether parameter is hidden (optional for set/add) |
| `x`, `y` | Position in schematic units (optional for add) |

**Operations:**

| Operation | Description |
|-----------|-------------|
| `list` | Returns all parameters for a symbol |
| `get` | Returns a single parameter by name |
| `set` | Updates an existing parameter's value |
| `add` | Creates a new parameter |
| `delete` | Removes a parameter |

### `manage_schlib_footprints`

Manage footprint links in Altium SchLib symbols. Supports listing, adding, and removing
footprint references that link schematic symbols to PCB footprints.

```json
{
    "name": "manage_schlib_footprints",
    "arguments": {
        "filepath": "./MyLibrary.SchLib",
        "component_name": "LM358",
        "operation": "add",
        "footprint_name": "SOIC-8_3.9x4.9mm"
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath` | Path to the SchLib file |
| `component_name` | Name of the symbol |
| `operation` | Operation: `list`, `add`, `remove` |
| `footprint_name` | Footprint name (required for add/remove) |
| `description` | Footprint description (optional for add) |

**Operations:**

| Operation | Description |
|-----------|-------------|
| `list` | Returns all linked footprints for a symbol |
| `add` | Links a new footprint to the symbol |
| `remove` | Removes a footprint link from the symbol |

### `repair_library`

Repair a library file by removing orphaned data. For PcbLib files, this removes component body
references that point to non-existent embedded STEP models. This fixes libraries where 3D model
data was corrupted or incompletely deleted.

```json
{
    "name": "repair_library",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "dry_run": true
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath` | Path to the library file (.PcbLib) |
| `dry_run` | If `true`, report what would be removed without modifying the file (default: `false`) |

**Example Response:**

```json
{
    "library_type": "PcbLib",
    "footprints_checked": 5,
    "orphaned_references_removed": [
        { "footprint_name": "RESC0603", "removed_count": 2 }
    ],
    "total_removed": 2,
    "dry_run": false
}
```

### `bulk_rename`

Rename multiple components in a library using pattern matching. Supports glob patterns and
regex with capture groups for flexible bulk renaming operations.

```json
{
    "name": "bulk_rename",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "pattern": "^RESC(.*)$",
        "replacement": "RES_$1",
        "pattern_type": "regex",
        "dry_run": true
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `filepath` | Path to the library file (.PcbLib or .SchLib) |
| `pattern` | Pattern to match component names |
| `replacement` | Replacement string (use `$1`, `$2`, etc. for regex capture groups) |
| `pattern_type` | Pattern type: `glob` or `regex` (default: `glob`) |
| `dry_run` | If `true`, preview changes without modifying the file (default: `false`) |

**Example Response:**

```json
{
    "renamed": [
        { "old_name": "RESC0402", "new_name": "RES_0402" },
        { "old_name": "RESC0603", "new_name": "RES_0603" }
    ],
    "skipped": [],
    "conflicts": [],
    "dry_run": true
}
```

**Pattern Examples:**

| Pattern Type | Pattern | Replacement | Input | Output |
|--------------|---------|-------------|-------|--------|
| glob | `RESC*` | `RES_` | `RESC0603` | `RES_0603` (suffix preserved) |
| regex | `^RESC(.*)$` | `RES_$1` | `RESC0603` | `RES_0603` |
| regex | `^(.*)_V(\d+)$` | `$1_REV$2` | `CAP_V2` | `CAP_REV2` |

### `component_exists`

Check if one or more components exist in an Altium library. Useful for validating references
before performing operations like copy or merge.

```json
{
    "name": "component_exists",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "component_names": ["RESC0603", "CAPC0402", "MISSING_COMPONENT"]
    }
}
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `filepath` | Yes | Path to the library file (.PcbLib or .SchLib) |
| `component_names` | Yes | Array of component names to check |

**Response:**

```json
{
    "status": "success",
    "filepath": "./MyLibrary.PcbLib",
    "file_type": "PcbLib",
    "results": [
        { "name": "RESC0603", "exists": true },
        { "name": "CAPC0402", "exists": true },
        { "name": "MISSING_COMPONENT", "exists": false }
    ],
    "all_exist": false,
    "found_count": 2,
    "missing_count": 1
}
```

### `update_pad`

Update specific properties of a pad in a PcbLib footprint without replacing the entire component.
Find the pad by designator and update only the specified properties.

```json
{
    "name": "update_pad",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "component_name": "RESC0603",
        "designator": "1",
        "updates": {
            "width": 1.0,
            "height": 0.9,
            "shape": "rectangle"
        },
        "dry_run": false
    }
}
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `filepath` | Yes | Path to the PcbLib file |
| `component_name` | Yes | Name of the footprint |
| `designator` | Yes | Pad designator to update (e.g., "1", "A1") |
| `updates` | Yes | Object with properties to update |
| `dry_run` | No | Preview changes without modifying the file (default: `false`) |

**Updatable Pad Properties:**

| Property | Description |
|----------|-------------|
| `x`, `y` | Position in mm |
| `width`, `height` | Pad dimensions in mm |
| `shape` | Pad shape: `rectangle`, `round`, `oval`, `rounded_rectangle` |
| `rotation` | Rotation angle in degrees |
| `hole_size` | Hole diameter in mm (for through-hole pads) |

**Response:**

```json
{
    "status": "success",
    "component_name": "RESC0603",
    "designator": "1",
    "changes": [
        { "property": "width", "old": 0.9, "new": 1.0 },
        { "property": "height", "old": 0.95, "new": 0.9 },
        { "property": "shape", "old": "RoundedRectangle", "new": "rectangle" }
    ]
}
```

### `update_primitive`

Update specific properties of a primitive (track, arc, text, fill, region) in a PcbLib footprint.
Find the primitive by type and index, and update only the specified properties.

```json
{
    "name": "update_primitive",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "component_name": "RESC0603",
        "primitive_type": "track",
        "index": 0,
        "updates": {
            "width": 0.15,
            "layer": "Top Overlay"
        },
        "dry_run": false
    }
}
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `filepath` | Yes | Path to the PcbLib file |
| `component_name` | Yes | Name of the footprint |
| `primitive_type` | Yes | Type: `track`, `arc`, `text`, `fill`, `region` |
| `index` | Yes | Zero-based index of the primitive in its type array |
| `updates` | Yes | Object with properties to update |
| `dry_run` | No | Preview changes without modifying the file (default: `false`) |

**Updatable Properties by Type:**

| Type | Properties |
|------|------------|
| `track` | `x1`, `y1`, `x2`, `y2`, `width`, `layer` |
| `arc` | `x`, `y`, `radius`, `start_angle`, `end_angle`, `width`, `layer` |
| `text` | `x`, `y`, `text`, `height`, `rotation`, `layer` |
| `fill` | `x1`, `y1`, `x2`, `y2`, `rotation`, `layer` |
| `region` | `layer` |

### `list_backups`

List available backup files for an Altium library. Backups are created automatically before
destructive operations.

```json
{
    "name": "list_backups",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib"
    }
}
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `filepath` | Yes | Path to the library file |

**Response:**

```json
{
    "status": "success",
    "filepath": "./MyLibrary.PcbLib",
    "backups": [
        {
            "filename": "MyLibrary.PcbLib.20260126_143022.bak",
            "path": "./MyLibrary.PcbLib.20260126_143022.bak",
            "timestamp": "2026-01-26T14:30:22",
            "size_bytes": 45678
        },
        {
            "filename": "MyLibrary.PcbLib.20260125_091500.bak",
            "path": "./MyLibrary.PcbLib.20260125_091500.bak",
            "timestamp": "2026-01-25T09:15:00",
            "size_bytes": 44123
        }
    ],
    "backup_count": 2
}
```

### `restore_backup`

Restore an Altium library from a backup file. If no specific backup is specified, restores
from the most recent backup.

```json
{
    "name": "restore_backup",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "backup_filename": "MyLibrary.PcbLib.20260125_091500.bak"
    }
}
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `filepath` | Yes | Path to the library file to restore |
| `backup_filename` | No | Specific backup filename to restore (default: most recent) |

**Response:**

```json
{
    "status": "success",
    "filepath": "./MyLibrary.PcbLib",
    "restored_from": "MyLibrary.PcbLib.20260125_091500.bak",
    "backup_timestamp": "2026-01-25T09:15:00",
    "message": "Restored './MyLibrary.PcbLib' from backup 'MyLibrary.PcbLib.20260125_091500.bak'"
}
```

---

## Primitive Types

### Footprint Primitives (PcbLib)

| Primitive | Description |
|-----------|-------------|
| **Pad** | SMD or through-hole pad with designator, position, size, shape, layer (see Pad Shapes below) |
| **Via** | Vertical interconnect with layer span, hole size, and thermal relief |
| **Track** | Line segment on any layer (silkscreen, assembly, etc.) |
| **Arc** | Arc or circle on any layer |
| **Region** | Filled polygon (courtyard, copper pour) |
| **Text** | Text string with font, size, position, layer |
| **Fill** | Filled rectangle on any layer |
| **ComponentBody** | 3D model reference (embedded STEP models) |

#### Pad Shapes and Pin 1 Indicator

The `shape` property on pads controls the copper shape. Use this to indicate pin 1:

| Shape | Value | Usage |
|-------|-------|-------|
| Rectangle | `"rectangle"` | **Pin 1 indicator** — use for the first pad to distinguish it visually |
| Rounded Rectangle | `"rounded_rectangle"` | Default for SMD pads (most common) |
| Round | `"round"` or `"circle"` | Circular pads, default for through-hole (both values are equivalent) |
| Oval | `"oval"` | Oblong pads for constrained spaces |

**Example — marking pin 1 with a rectangular pad:**

```json
{
    "pads": [
        { "designator": "1", "x": -0.75, "y": 0, "width": 0.9, "height": 0.95, "shape": "rectangle" },
        { "designator": "2", "x": 0.75, "y": 0, "width": 0.9, "height": 0.95, "shape": "rounded_rectangle" }
    ]
}
```

This follows the IPC-7351 convention where pin 1 has a distinct shape (typically rectangular or square corners) while other pads use rounded corners.

### Symbol Primitives (SchLib)

| Primitive | Description |
|-----------|-------------|
| **Pin** | Component pin with name, designator, electrical type, orientation |
| **Rectangle** | Filled or unfilled rectangle (component body) |
| **RoundRect** | Rounded rectangle with corner radii |
| **Line** | Single line segment |
| **Polyline** | Multiple connected line segments |
| **Polygon** | Filled polygon with border and fill colours |
| **Arc** | Arc or circle |
| **Ellipse** | Ellipse or circle (filled or unfilled) |
| **EllipticalArc** | Elliptical arc segment with fractional radii |
| **Bezier** | Cubic Bezier curve (4 control points) |
| **Label** | Text label (RECORD=4) |
| **Text** | Text annotation (RECORD=3) |
| **Parameter** | Component parameter (Value, Part Number, etc.) |
| **FootprintModel** | Reference to a footprint in a PcbLib |

### Standard Altium Layers

Common layers for footprints (each has a Bottom equivalent):

| Layer | Usage |
|-------|-------|
| Top Layer | Copper pads (SMD) |
| Bottom Layer | Bottom copper pads |
| Multi-Layer | Through-hole pads (all copper layers) |
| Top Overlay | Silkscreen |
| Top Paste | Solder paste stencil |
| Top Solder | Solder mask openings |
| Top Assembly | Assembly outline (documentation) |
| Top Courtyard | Courtyard boundary (IPC-7351) |
| Top 3D Body | 3D model outline |

Additional layers supported:

| Layer | Usage |
|-------|-------|
| Mid-Layer 1–30 | Internal copper layers |
| Internal Plane 1–16 | Power/ground planes |
| Mechanical 1–16 | User-defined mechanical layers |
| Drill Guide | Drill hole markers |
| Drill Drawing | Drill chart/table |
| Keep-Out Layer | Routing exclusion zones |

---

## Installation

See [CONTRIBUTING.md § Development Setup](CONTRIBUTING.md#development-setup) for build instructions.

The release binary will be at `target/release/altium-designer-mcp`.

### Command-Line Usage

```bash
altium-designer-mcp [OPTIONS] [CONFIG_FILE]
```

| Option | Description |
|--------|-------------|
| `CONFIG_FILE` | Path to configuration file (optional, uses default location if omitted) |
| `-v`, `--verbose` | Increase logging verbosity (`-v` info, `-vv` debug, `-vvv` trace) |
| `-q`, `--quiet` | Decrease logging verbosity (only show errors) |
| `-h`, `--help` | Print help information |
| `-V`, `--version` | Print version information |

### Usage with Claude Desktop

Add to your Claude Desktop MCP configuration:

```json
{
    "mcpServers": {
        "altium": {
            "command": "altium-designer-mcp",
            "args": ["/path/to/config.json"]
        }
    }
}
```

---

## Configuration

Configuration file location:

- **Linux/macOS:** `~/.altium-designer-mcp/config.json`
- **Windows:** `%USERPROFILE%\.altium-designer-mcp\config.json`

```json
{
    "allowed_paths": [
        "/path/to/your/altium/libraries",
        "/another/library/path"
    ],
    "logging": {
        "level": "warn"
    }
}
```

### Configuration Options

| Option | Description |
|--------|-------------|
| `allowed_paths` | Array of directory paths where library files can be accessed (default: current directory) |
| `logging.level` | Log level: trace, debug, info, warn, error (default: warn) |

---

## STEP Model Integration

STEP models are **attached**, not generated. The tool links existing STEP files to footprints.

```json
{
    "step_model": {
        "filepath": "./3d-models/0603.step",
        "x_offset": 0,
        "y_offset": 0,
        "z_offset": 0,
        "rotation": 0
    }
}
```

### Embedded vs External Models

Altium supports two ways to reference 3D models:

| Type | Storage | Portability |
|------|---------|-------------|
| **Embedded** | STEP data stored inside the .PcbLib file | Fully portable — the model travels with the library |
| **External** | File path reference to a .step file on disk | Not portable — requires the file to exist at the referenced path |

When using `copy_component_cross_library` or `merge_libraries`:

- **Embedded models** are copied along with the component
- **External model references** are removed with a warning, as the file paths are not portable across different machines or directory structures

To preserve 3D models when copying components, ensure they are embedded in the source library (not external references).

### Extracting Embedded Models

Use `extract_step_model` to extract embedded STEP data from a library:

```json
{
    "name": "extract_step_model",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "output_path": "./extracted_model.step"
    }
}
```

For parametric 3D model generation, a dedicated mechanical MCP server is planned as a future project.

---

## Automatic Backups

Before any destructive operation (delete, update, merge, batch update), the server automatically
creates a timestamped backup of the target file. Backups use the format:

```text
MyLibrary.PcbLib.20260125_143022.bak
```

**Backup retention:** Only the 5 most recent backups per file are kept. Older backups are
automatically removed to prevent unbounded disk usage.

**Operations that create backups:**

- `delete_component`
- `update_component`
- `update_pad`
- `update_primitive`
- `rename_component`
- `copy_component`
- `copy_component_cross_library` (target file)
- `merge_libraries` (target file)
- `reorder_components`
- `batch_update`
- `bulk_rename`
- `write_pcblib` / `write_schlib` (when overwriting)
- `import_library` (when overwriting)

**Disabling backups:** All write operations accept a `create_backup` parameter (default: `true`).
Set to `false` to skip backup creation:

```json
{
    "name": "delete_component",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "component_names": ["OLD_COMPONENT"],
        "create_backup": false
    }
}
```

**Managing backups:** Use `list_backups` to view available backups and `restore_backup` to
recover from a previous version.

**Dry-run support:** Most destructive operations support `dry_run=true` to preview changes
without modifying files:

- `delete_component` — preview which components would be deleted
- `update_component` — preview component replacement changes
- `update_pad` / `update_primitive` — preview property changes
- `bulk_rename` — preview name changes
- `repair_library` — preview orphaned references to remove
- `copy_component` / `rename_component` / `reorder_components`
- `write_pcblib` / `write_schlib` / `import_library`
- `copy_component_cross_library` / `merge_libraries`

---

## Notes

### Long Component Names

Component names longer than 31 characters are supported. The OLE Compound File format limits
storage names to 31 characters, so longer names are automatically truncated internally while
the full name is preserved in component parameters. This is handled transparently — you can
use any length component name and it will be preserved on read/write roundtrips.

---

## Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

- Follow the style guide in [STYLE.md](STYLE.md)
- Security issues: see [SECURITY.md](SECURITY.md)

---

## Development

### Running Tests

```bash
cargo test
```

Tests are self-contained and generate their own data programmatically. Temporary files are created in `.tmp/` (git-ignored) and automatically cleaned up.

### Code Quality

```bash
cargo fmt --check  # Check formatting
cargo clippy       # Lint
```

---

## Licence

Copyright (C) 2026 The Embedded Society <https://github.com/embedded-society/altium-designer-mcp>.

GNU General Public License v3.0 — see [LICENCE](LICENCE).

---

## Links

- [MCP Specification](https://modelcontextprotocol.io/)
- [Report an Issue](https://github.com/embedded-society/altium-designer-mcp/issues)

---

## Sample Files

Sample Altium library files are included in the `scripts/` folder for **manual debugging only**.
Automated tests do not depend on these files.

See [scripts/README.md](scripts/README.md) for details on available sample files and analysis scripts.

---

## Prior Art

This project builds on the work of:

- [AltiumSharp](https://github.com/issus/AltiumSharp) — C# Altium file parser (MIT)
- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) — Python Altium library
- [python-altium](https://github.com/vadmium/python-altium) — Altium format documentation
