<!-- GENERATED — do not edit by hand.
     Source of truth: src/mcp/tool_definitions.rs
     Regenerate: UPDATE_DOCS=1 cargo test --lib tools_md_in_sync -->

<!-- markdownlint-disable MD013 -->
<!-- Generated tables and inline JSON schemas legitimately exceed the line-length limit. -->

# MCP Tools Reference

Every tool the **altium-designer-mcp** server exposes, rendered from the tool
definitions served over `tools/list`. Coordinates are millimetres for `.PcbLib`
footprints and schematic units (10 units = 1 grid square) for `.SchLib` symbols.

_34 tools._

## `read_pcblib`

**Read a PcbLib** — read-only

Read an Altium .PcbLib file and return its contents including footprints with their primitives (pads, vias, tracks, arcs, regions, fills, text, component_bodies). Returns
structured data that can be used to understand existing footprint styles. All coordinates and dimensions are in millimetres (mm). Fields such as guid, unique_id,
raw_tail, raw_block, raw_geometry, raw_layer_id, additional_parameters, param_key_order, primitive_order and storage_name are fidelity carriers: pass them back unchanged
to write_pcblib or update_component and the rewrite is byte-identical to the source; omit them when authoring from scratch. Each footprint is the same JSON shape
get_component, export_library and write_pcblib use; a list with no entries and an optional field with no value are omitted rather than empty/null. For large libraries,
use component_name to fetch specific footprints, or use limit/offset for pagination.

**Example**

```json
{
    "arguments": {
        "filepath": "./MyLibrary.PcbLib"
    },
    "name": "read_pcblib"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `compact` | boolean | no | If true (default), omit per-layer pad data when stack_mode is Simple. Set to false for full output. |
| `component_name` | string | no | Optional: fetch only this footprint, by name in any case; a name the library does not hold is an error naming the available footprints |
| `filepath` | string | yes | Path to the .PcbLib file |
| `limit` | integer | no | Optional: maximum number of footprints to return, 1 or more (default: all) (min: 1) |
| `offset` | integer | no | Optional: skip the first N footprints, 0 or more (default: 0) (min: 0) |

## `read_schlib`

**Read a SchLib** — read-only

Read an Altium .SchLib file and return its contents including symbols with their primitives (pins, rectangles, round_rects, lines, polylines, polygons, arcs, pies,
images, text_frames, beziers, ellipses, elliptical_arcs, labels, ieee_symbols), parameters and footprint links. Coordinates are in schematic units (10 units = 1 grid
square, not mm). Fields such as unique_id, primitive_order, header_params, raw_params, all_pin_count, extra_streams and storage_name are fidelity carriers: pass them back
unchanged to write_schlib or update_component and the rewrite is byte-identical to the source; omit them when authoring from scratch. Each symbol is the same JSON shape
get_component, export_library and write_schlib use; a list with no entries and an optional field with no value are omitted rather than empty/null. For large libraries,
use component_name to fetch specific symbols, or use limit/offset for pagination.

**Example**

```json
{
    "arguments": {
        "filepath": "./MySymbols.SchLib"
    },
    "name": "read_schlib"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_name` | string | no | Optional: fetch only this symbol, by name in any case; a name the library does not hold is an error naming the available symbols |
| `filepath` | string | yes | Path to the .SchLib file |
| `limit` | integer | no | Optional: maximum number of symbols to return, 1 or more (default: all) (min: 1) |
| `offset` | integer | no | Optional: skip the first N symbols, 0 or more (default: 0) (min: 0) |

## `list_components`

**List components** — read-only

List all component/footprint names in an Altium library file (.PcbLib or .SchLib). Supports pagination with limit/offset for large libraries. Use include_metadata for
additional details like part_count and pin_count.

**Example**

```json
{
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "include_metadata": true,
        "limit": 50,
        "offset": 0
    },
    "name": "list_components"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `filepath` | string | yes | Path to the library file |
| `include_metadata` | boolean | no | If true, return objects with metadata instead of just names: a footprint's description, one count per primitive kind and has_3d_model; a symbol's description, designator, part_count, pin_count and footprint_count. Default: false |
| `limit` | integer | no | Maximum number of components to return, 1 or more (optional, default: all) (min: 1) |
| `offset` | integer | no | Number of components to skip, 0 or more (optional, default: 0) (min: 0) |

## `extract_style`

**Extract a library's style** — read-only

Extract style information from an existing Altium library file. A PcbLib reports track and arc widths per layer, pad shapes, text heights and the layer usage of every
primitive kind (a via counts as Multi-Layer); a SchLib reports pin lengths, stroke widths and the stroke, fill and text colours of every record kind. Use this to learn
from existing libraries and create consistent new components.

**Example**

```json
{
    "arguments": {
        "filepath": "./MyLibrary.PcbLib"
    },
    "name": "extract_style"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `filepath` | string | yes | Path to the .PcbLib or .SchLib file |

## `write_pcblib`

**Write a PcbLib** — writes files (a backup is taken first)

Write footprints to an Altium .PcbLib file (set 'append': true to add to an existing library instead of replacing it). Each footprint is defined by its primitives: pads
(with position, size, shape, layer), tracks, vias, fills, arcs, regions, text and component_bodies. The AI is responsible for calculating correct positions and sizes
based on IPC-7351B or other standards. All coordinates and dimensions must be in millimetres (mm). A footprint authored without a '.Designator' text receives one on the
Top Overlay automatically, just above its topmost pad, so every placed part shows its reference designator: supply your own to control its placement, or set
'auto_designator': false to omit it; a footprint echoed back from a read (carrying primitive_order) is never touched. The response 'bodies' array echoes each footprint's
3D body height and source; a footprint with no STEP model and no component body reports source 'none'. Set 'auto_3d_body': true to have an extruded placeholder body
(default height 1.0 mm, flagged 'assumed_height': true) added to such footprints, then confirm or override it by supplying 'component_bodies' explicitly. The response
also includes a 'warnings' array flagging silkscreen (overlay) tracks that overlap a pad (silk-on-pad) so you can move them clear. No text field may contain '|', the
separator of Altium's record format, which cannot hold it.

**Example**

```json
{
    "arguments": {
        "append": false,
        "filepath": "./Passives.PcbLib",
        "footprints": [
            {
                "description": "Chip resistor, 0603 (1608 metric)",
                "name": "RESC1608X55N",
                "pads": [
                    {
                        "designator": "1",
                        "height": 0.95,
                        "width": 0.9,
                        "x": -0.75,
                        "y": 0
                    },
                    {
                        "designator": "2",
                        "height": 0.95,
                        "width": 0.9,
                        "x": 0.75,
                        "y": 0
                    }
                ],
                "regions": [
                    {
                        "layer": "Top Courtyard",
                        "vertices": [
                            {
                                "x": -1.45,
                                "y": -0.73
                            },
                            {
                                "x": 1.45,
                                "y": -0.73
                            },
                            {
                                "x": 1.45,
                                "y": 0.73
                            },
                            {
                                "x": -1.45,
                                "y": 0.73
                            }
                        ]
                    }
                ],
                "tracks": [
                    {
                        "layer": "Top Overlay",
                        "width": 0.12,
                        "x1": -0.8,
                        "x2": 0.8,
                        "y1": -0.425,
                        "y2": -0.425
                    },
                    {
                        "layer": "Top Overlay",
                        "width": 0.12,
                        "x1": -0.8,
                        "x2": 0.8,
                        "y1": 0.425,
                        "y2": 0.425
                    }
                ]
            }
        ]
    },
    "name": "write_pcblib"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `append` | boolean | no | If true, append to existing file; if false, create new file |
| `auto_3d_body` | boolean | no | If true, footprints with pads but no STEP model and no component body get a placeholder extruded 3D body (1.0 mm tall, flagged assumed_height). Default false: nothing is added unless you ask, since many footprints (fiducials, test points, mounting holes) legitimately have no body. Prefer supplying real heights via component_bodies. |
| `auto_designator` | boolean | no | If true (default), a footprint authored without a '.Designator' text gets one on the Top Overlay just above its topmost pad, so the placed part shows its reference designator. Never applied to a footprint echoed back from read_pcblib/get_component (one carrying primitive_order): Altium's own library footprints carry no designator text, and a read-modify-write must not add primitives. Set false to author a footprint without one. |
| `filepath` | string | yes | Path to the .PcbLib file to create/modify |
| `footprints` | array<object> | yes | Array of footprint definitions |

## `write_schlib`

**Write a SchLib** — writes files (a backup is taken first)

Write schematic symbols to an Altium .SchLib file (set 'append': true to add to an existing library instead of replacing it). Each symbol is defined by its primitives:
pins, rectangles, round_rects, lines, polylines, polygons, arcs, pies, images, text_frames, beziers, ellipses, elliptical_arcs, labels, and text — plus its designator,
parameters (Value, Manufacturer, ...) and footprint links ('footprints', name + optional library_path). Multi-part symbols set 'part_count' and tag each pin with
'owner_part_id'. Coordinates must be in schematic units (10 units = 1 grid square, not mm); a pin's (x, y) is its body-attach end and 'orientation' is the direction it
points outward — the response echoes each pin's computed body_end and tip. No text field may contain '|', the separator of Altium's record format; Altium's own editor
stores it as '¦' (U+00A6).

**Example**

```json
{
    "arguments": {
        "filepath": "./MyLibrary.SchLib",
        "symbols": [
            {
                "designator_prefix": "R",
                "footprints": [
                    {
                        "library_path": "./MyLibrary.PcbLib",
                        "name": "R0402"
                    }
                ],
                "name": "R",
                "parameters": [
                    {
                        "name": "Value",
                        "value": "10k"
                    }
                ],
                "pins": [
                    {
                        "designator": "1",
                        "electrical_type": "passive",
                        "length": 20,
                        "name": "1",
                        "orientation": "left",
                        "x": -50,
                        "y": 0
                    },
                    {
                        "designator": "2",
                        "electrical_type": "passive",
                        "length": 20,
                        "name": "2",
                        "orientation": "right",
                        "x": 50,
                        "y": 0
                    }
                ],
                "rectangles": [
                    {
                        "x1": -50,
                        "x2": 50,
                        "y1": -20,
                        "y2": 20
                    }
                ]
            }
        ]
    },
    "name": "write_schlib"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `append` | boolean | no | If true, append to existing file; if false, create new file |
| `filepath` | string | yes | Path to the .SchLib file to create/modify |
| `symbols` | array<object> | yes | Array of symbol definitions |

## `write_libpkg`

**Write a LibPkg project** — writes files (a backup is taken first)

Write an Altium Library Package (.LibPkg) project file that groups source library documents (.SchLib and .PcbLib) so they can be compiled into an Integrated Library
(.IntLib). Member documents are referenced by their path relative to the .LibPkg. This generates only the project source; compiling to a binary .IntLib is a one-click
operation inside Altium Designer (Project > Compile Integrated Library).

**Example**

```json
{
    "arguments": {
        "documents": [
            "MyLibrary.SchLib",
            "MyLibrary.PcbLib"
        ],
        "filepath": "./MyLibrary.LibPkg"
    },
    "name": "write_libpkg"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `documents` | array<string> | yes | Member document paths (.SchLib / .PcbLib). Each is referenced relative to the .LibPkg location; same-folder files become bare names. |
| `filepath` | string | yes | Path to the .LibPkg file to create |

## `delete_component`

**Delete components** — writes files (a backup is taken first)

Delete one or more components from an Altium library file (.PcbLib or .SchLib). The file type is auto-detected from the extension. Returns status for each component:
deleted, not_found, or error. Use dry_run=true to preview changes without modifying the file.

**Example**

```json
{
    "arguments": {
        "component_names": [
            "OLD_FOOTPRINT",
            "UNUSED_COMPONENT"
        ],
        "dry_run": false,
        "filepath": "./MyLibrary.PcbLib"
    },
    "name": "delete_component"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_names` | array<string> | yes | Names of components to delete |
| `dry_run` | boolean | no | If true, show what would be deleted without actually modifying the file (default: `false`) |
| `filepath` | string | yes | Path to the .PcbLib or .SchLib file |

## `validate_library`

**Validate a library** — read-only

Validate an Altium library file for common issues. Checks for: empty components (no pads/pins), duplicate designators, invalid coordinates, zero-size primitives,
overlapping pads, 3D bodies whose embedded model the library does not contain, embedded models no footprint references, and other integrity problems. Returns a list of
warnings and errors.

**Example**

```json
{
    "arguments": {
        "filepath": "./MyLibrary.PcbLib"
    },
    "name": "validate_library"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `filepath` | string | yes | Path to the .PcbLib or .SchLib file |

## `export_library`

**Export a library** — read-only

Export a .PcbLib or .SchLib as text in the response — nothing is written to disk. JSON carries every component in the same shape read_pcblib/read_schlib and
write_pcblib/write_schlib use, plus for a PcbLib the embedded 3D models its bodies reference (`embedded_models`, base64 STEP data keyed by model GUID), so import_library
can rebuild the library elsewhere; CSV is a summary table: name, description (and a symbol's designator), one count column per primitive kind, and the external 3D model /
footprint link count. Use JSON to version-control, back up or move a library and CSV for an inventory; use read_pcblib/read_schlib instead to inspect components with
pagination, and list_components with details for a quick overview.

**Example**

```json
{
    "arguments": {
        "compact": true,
        "filepath": "./MyLibrary.PcbLib",
        "format": "json"
    },
    "name": "export_library"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `compact` | boolean | no | For PcbLib JSON export: if true (default), omit per-layer pad data when stack_mode is Simple |
| `filepath` | string | yes | Path to the .PcbLib or .SchLib file |
| `format` | enum | yes | Export format: 'json' for full data, 'csv' for summary table (one of: json, csv) |

## `import_library`

**Import a library** — writes files (a backup is taken first)

Import components from JSON data into an Altium library file. Accepts JSON in the format produced by export_library, enabling round-trip workflows: a PcbLib export's
`embedded_models` are restored alongside the footprints, and a body whose model the data does not contain is reported in warnings. Auto-detects library type
(PcbLib/SchLib) from the JSON data.

**Example**

```json
{
    "arguments": {
        "json_data": {
            "file_type": "PcbLib",
            "footprints": [
                {
                    "name": "R0402",
                    "pads": []
                }
            ]
        },
        "output_path": "./MyLibrary.PcbLib"
    },
    "name": "import_library"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `append` | boolean | no | If true, append to existing library instead of overwriting. Default: false |
| `json_data` | object | yes | JSON data containing components to import. Should have 'file_type' (PcbLib/SchLib) and 'footprints' or 'symbols' array. |
| `output_path` | string | yes | Path where the new library file will be created (.PcbLib or .SchLib) |

## `extract_step_model`

**Extract a STEP model** — read-only

Extract embedded STEP 3D models from an Altium .PcbLib file. Models are stored compressed inside the library and this tool extracts them to standalone .step files.
Supports multiple modes: 'auto' (default), 'list', 'extract_all', or 'extract_by_footprint'.

**Example**

```json
{
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "mode": "auto",
        "model": "RESC1005X04L.step",
        "output_path": "./extracted_model.step"
    },
    "name": "extract_step_model"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `filepath` | string | yes | Path to the .PcbLib file containing embedded 3D models |
| `footprint_name` | string | no | Footprint name to extract models for (required for 'extract_by_footprint' mode) |
| `limit` | integer | no | Maximum number of models to list, 1 or more (for 'list' mode) (min: 1) |
| `mode` | enum | no | Extraction mode: 'auto' (default) extracts single model or lists if multiple; 'list' always lists models; 'extract_all' extracts all models to output_dir; 'extract_by_footprint' extracts models used by specified footprint (one of: auto, list, extract_all, extract_by_footprint) |
| `model` | string | no | Model name (e.g., 'RESC1005X04L.step') or GUID to extract (for 'auto' mode) |
| `offset` | integer | no | Number of models to skip when listing, 0 or more (for 'list' mode) (min: 0) |
| `output_path` | string | no | Meaning depends only on the mode, never on how many models match: for 'auto' it is the FILE path for the extracted .step; for 'extract_all' and 'extract_by_footprint' it is a DIRECTORY that receives one file per model (created if absent). Omit to get the model inline as base64 ('auto' single model, or 'extract_by_footprint' with a single match). |

## `diff_libraries`

**Compare two libraries** — read-only

Compare two Altium library files and report differences. Shows added, removed, and modified components. Both files must be the same type (.PcbLib or .SchLib).

**Example**

```json
{
    "arguments": {
        "filepath_a": "./OldLibrary.PcbLib",
        "filepath_b": "./NewLibrary.PcbLib"
    },
    "name": "diff_libraries"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `filepath_a` | string | yes | Path to the first (base/old) library file |
| `filepath_b` | string | yes | Path to the second (new/changed) library file |

## `batch_update`

**Batch-update a library** — writes files (a backup is taken first)

Perform one batch operation across all components in an Altium library file. PcbLib: 'update_track_width' (change every track of from_width to to_width, within tolerance)
and 'rename_layer' (move every primitive from from_layer to to_layer). SchLib: 'update_parameters' (set parameter values across symbols). Use dry_run=true to preview
changes without modifying the file.

**Example**

```json
{
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "operation": "update_track_width",
        "parameters": {
            "from_width": 0.2,
            "to_width": 0.25,
            "tolerance": 0.001
        }
    },
    "name": "batch_update"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `dry_run` | boolean | no | If true, show what would be updated without actually modifying the file (default: `false`) |
| `filepath` | string | yes | Path to the Altium library file (.PcbLib or .SchLib) |
| `operation` | enum | yes | The batch operation to perform. PcbLib: update_track_width, rename_layer. SchLib: update_parameters. (one of: update_track_width, rename_layer, update_parameters) |
| `parameters` | object | yes | Operation-specific parameters |

## `copy_component`

**Duplicate a component** — writes files (a backup is taken first)

Copy/duplicate a component within an Altium library file. Creates a new component with a different name and identical primitives, but its own identity: the copy's GUIDs
and unique ids are minted fresh rather than shared with the original. Useful for creating variants.

**Example**

```json
{
    "arguments": {
        "description": "0603 resistor variant 2",
        "filepath": "./MyLibrary.PcbLib",
        "source_name": "RESC0603_IPC_MEDIUM",
        "target_name": "RESC0603_IPC_MEDIUM_V2"
    },
    "name": "copy_component"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `description` | string | no | Optional description for the new component (defaults to source description) |
| `dry_run` | boolean | no | If true, validate the operation without modifying the file. Default: false |
| `filepath` | string | yes | Path to the Altium library file (.PcbLib or .SchLib) |
| `source_name` | string | yes | Name of the component to copy |
| `target_name` | string | yes | Name for the new copied component |

## `rename_component`

**Rename a component** — writes files (a backup is taken first)

Rename a component within an Altium library file. This is an atomic operation that changes the component's name while preserving all primitives and properties. More
efficient than copy + delete for simple renames.

**Example**

```json
{
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "new_name": "RESC0603_NEW",
        "old_name": "RESC0603_OLD"
    },
    "name": "rename_component"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `dry_run` | boolean | no | If true, validate the operation without modifying the file. Default: false |
| `filepath` | string | yes | Path to the Altium library file (.PcbLib or .SchLib) |
| `new_name` | string | yes | New name for the component |
| `old_name` | string | yes | Current name of the component to rename |

## `copy_component_cross_library`

**Copy a component to another library** — writes files (a backup is taken first)

Copy a component from one Altium library to another. Both libraries must be the same type (PcbLib to PcbLib, or SchLib to SchLib), and different files (use copy_component
to duplicate within a library). The component keeps its identity, and the embedded 3D models its bodies reference travel with it; an external STEP file reference is
dropped with a warning unless preserve_external_paths is true, since a path relative to the source library rarely resolves elsewhere. Useful for consolidating libraries
or sharing components between projects.

**Example**

```json
{
    "arguments": {
        "component_name": "RESC0603_IPC_MEDIUM",
        "description": "Copied from SourceLibrary",
        "ignore_missing_models": false,
        "new_name": "RESC0603_COPIED",
        "preserve_external_paths": false,
        "source_filepath": "./SourceLibrary.PcbLib",
        "target_filepath": "./TargetLibrary.PcbLib"
    },
    "name": "copy_component_cross_library"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_name` | string | yes | Name of the component to copy from the source library |
| `description` | string | no | Optional new description for the component (defaults to original description) |
| `ignore_missing_models` | boolean | no | If true, copy the component even if referenced embedded 3D models are missing (PcbLib only). The component body references will be removed. Defaults to false. |
| `new_name` | string | no | Optional new name for the component in the target library (defaults to original name) |
| `preserve_external_paths` | boolean | no | If true, preserve external 3D model paths (model_3d field) instead of removing them. The path may need manual adjustment in the target location. Defaults to false. |
| `source_filepath` | string | yes | Path to the source library file (.PcbLib or .SchLib) |
| `target_filepath` | string | yes | Path to the target library file (must be same type as source) |

## `merge_libraries`

**Merge libraries** — writes files (a backup is taken first)

Merge multiple Altium libraries into a single library. All source libraries must be the same type (all PcbLib or all SchLib). Components are copied from each source into
the target library, together with the embedded 3D models their bodies reference (a model shared by several footprints is copied once; a body whose model is missing from
its source is merged as-is and reported in warnings). External STEP file references are carried unchanged. Use dry_run=true to preview what would be merged.

**Example**

```json
{
    "arguments": {
        "on_duplicate": "skip",
        "source_filepaths": [
            "./LibraryA.PcbLib",
            "./LibraryB.PcbLib",
            "./LibraryC.PcbLib"
        ],
        "target_filepath": "./MergedLibrary.PcbLib"
    },
    "name": "merge_libraries"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `dry_run` | boolean | no | If true, show what would be merged without actually modifying any files (default: `false`) |
| `on_duplicate` | enum | no | How to handle duplicate component names: 'skip' (ignore duplicates), 'error' (fail on duplicates), 'rename' (auto-rename with suffix). Default: 'error' (one of: skip, error, rename) |
| `source_filepaths` | array<string> | yes | Array of paths to source library files (.PcbLib or .SchLib) |
| `target_filepath` | string | yes | Path to the target library file (will be created or appended to) |

## `reorder_components`

**Reorder components** — writes files (a backup is taken first)

Reorder components in an Altium library file (.PcbLib or .SchLib). Specify the desired order as a list of component names. Components not in the list are placed at the
end in their original relative order.

**Example**

```json
{
    "arguments": {
        "component_order": [
            "RESC1608X55N",
            "RESC0805X40N",
            "RESC0402X20N"
        ],
        "filepath": "./MyLibrary.PcbLib"
    },
    "name": "reorder_components"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_order` | array<string> | yes | Component names in desired order |
| `filepath` | string | yes | Path to the .PcbLib or .SchLib file |

## `update_component`

**Update a component in place** — writes files (a backup is taken first)

Update a component in-place within an Altium library file, preserving its position. For PcbLib, provide a footprint object. For SchLib, provide a symbol object. The
component is matched by name. Use dry_run=true to preview changes without modifying.

**Example**

```json
{
    "arguments": {
        "component_name": "RESC0402X20N",
        "filepath": "./MyLibrary.PcbLib",
        "footprint": {
            "description": "Updated resistor 0402",
            "name": "RESC0402X20N",
            "pads": [
                {
                    "designator": "1",
                    "height": 0.5,
                    "layer": "TopLayer",
                    "width": 0.5,
                    "x": -0.5,
                    "y": 0
                },
                {
                    "designator": "2",
                    "height": 0.5,
                    "layer": "TopLayer",
                    "width": 0.5,
                    "x": 0.5,
                    "y": 0
                }
            ]
        }
    },
    "name": "update_component"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_name` | string | yes | Name of the component to update (must exist in library) |
| `dry_run` | boolean | no | If true, show what would be updated without actually modifying the file (default: `false`) |
| `filepath` | string | yes | Path to the .PcbLib or .SchLib file |
| `footprint` | object | no | For PcbLib: the footprint to store, in the shape write_pcblib takes per footprints entry |
| `symbol` | object | no | For SchLib: the symbol to store, in the shape write_schlib takes per symbols entry |

## `search_components`

**Search components** — read-only

Search for components across multiple Altium libraries using regex or glob patterns. Returns matching component names with their source library paths. Supports both
`.PcbLib` (footprints) and `.SchLib` (symbols) files.

**Example**

```json
{
    "arguments": {
        "filepaths": [
            "./Resistors.PcbLib",
            "./Capacitors.PcbLib",
            "./ICs.PcbLib"
        ],
        "pattern": "SOIC-*",
        "pattern_type": "glob"
    },
    "name": "search_components"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `filepaths` | array<string> | yes | Array of library file paths to search (.PcbLib or .SchLib) |
| `pattern` | string | yes | Search pattern to match component names |
| `pattern_type` | enum | no | Pattern type: 'glob' (wildcards like * and ?) or 'regex' (regular expressions). Default: 'glob' (one of: glob, regex) |

## `get_component`

**Get a component** — read-only

Get a single component by name from an Altium library. Returns the full component data (footprint or symbol) without needing to read and filter the entire library.
Supports both `.PcbLib` (footprints) and `.SchLib` (symbols) files.

**Example**

```json
{
    "arguments": {
        "component_name": "SOIC-8",
        "filepath": "./MyLibrary.PcbLib"
    },
    "name": "get_component"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_name` | string | yes | Exact name of the component to retrieve |
| `filepath` | string | yes | Path to the Altium library file (.PcbLib or .SchLib) |

## `component_exists`

**Check components exist** — read-only

Check if one or more components exist in an Altium library. Use this to validate component names before operations like rename, copy, or delete. Supports both `.PcbLib`
and `.SchLib` files.

**Example**

```json
{
    "arguments": {
        "component_names": [
            "RESC0603",
            "CAPC0402",
            "MISSING_COMPONENT"
        ],
        "filepath": "./MyLibrary.PcbLib"
    },
    "name": "component_exists"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_names` | array<string> | yes | List of component names to check |
| `filepath` | string | yes | Path to the Altium library file (.PcbLib or .SchLib) |

## `render_footprint`

**Preview a footprint** — read-only

Render an ASCII art visualisation of a footprint from a PcbLib file: every primitive kind — pads (with designators), vias, tracks, arcs, fills, regions, text marks and
3D-body outlines — each with its own marker, plus a per-kind count line and a legend. A quick preview, not a rendering.

**Example**

```json
{
    "arguments": {
        "component_name": "RESC0603_IPC_MEDIUM",
        "filepath": "./MyLibrary.PcbLib",
        "max_height": 40,
        "max_width": 80,
        "scale": 2.0
    },
    "name": "render_footprint"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_name` | string | yes | Name of the footprint to render |
| `filepath` | string | yes | Path to the Altium PcbLib file |
| `max_height` | integer | no | Maximum height in characters (default: 40) (min: 1) |
| `max_width` | integer | no | Maximum width in characters (default: 80) (min: 1) |
| `scale` | number | no | Characters per mm (default: 2.0). Higher = more detail |

## `render_symbol`

**Preview a symbol** — read-only

Render an ASCII-art preview of one symbol in a .SchLib — a quick visual check of a layout after authoring or editing, not a drawing to keep. Every record kind of the
requested part — pins (with designators), rectangles, rounded rectangles, lines, polylines, polygons, arcs, pies, ellipses, elliptical arcs, beziers, images, text frames,
labels and IEEE symbols — gets its own marker, followed by a per-kind count line and a legend. part_id picks one part of a multi-part symbol (default 1; 0 draws all
parts); scale sets characters per 10 schematic units and the picture is limited to max_width x max_height characters. Coordinates are schematic units (10 units = 1 grid).
Use get_component or read_schlib for exact coordinates and properties, and render_footprint for a .PcbLib footprint.

**Example**

```json
{
    "arguments": {
        "component_name": "LM358",
        "filepath": "./MyLibrary.SchLib",
        "max_height": 40,
        "max_width": 80,
        "part_id": 1,
        "scale": 1.0
    },
    "name": "render_symbol"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_name` | string | yes | Name of the symbol to render |
| `filepath` | string | yes | Path to the Altium SchLib file |
| `max_height` | integer | no | Maximum height in characters (default: 40) (min: 1) |
| `max_width` | integer | no | Maximum width in characters (default: 80) (min: 1) |
| `part_id` | integer | no | Part ID for multi-part symbols (default: 1, shows all parts if 0) (min: 0) |
| `scale` | number | no | Characters per 10 schematic units (default: 1.0). Higher = more detail |

## `manage_schlib_parameters`

**Manage symbol parameters** — writes files (a backup is taken first)

Manage the parameters of one symbol in a .SchLib (Value, Manufacturer, Part Number and the like): list them, get one, set an existing one, add a new one, or delete one.
Names match without regard to case, as Altium treats them; set refuses a name the symbol lacks (use add) and add refuses one it already has (use set). Every change backs
the library up to a timestamped .bak beside it (the five newest are kept) before saving, and the reply carries the parameter as stored; list returns every parameter with
a count. Use get_component or read_schlib to see parameters together with the symbol's pins and graphics, update_component to rewrite a symbol wholesale, and
manage_schlib_footprints for footprint links, which are models rather than parameters.

**Example**

```json
{
    "arguments": {
        "component_name": "LM358",
        "filepath": "./MyLibrary.SchLib",
        "operation": "set",
        "parameter_name": "Value",
        "value": "LM358D"
    },
    "name": "manage_schlib_parameters"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_name` | string | yes | Name of the symbol to manage parameters for |
| `filepath` | string | yes | Path to the Altium SchLib file |
| `hidden` | boolean | no | Whether the parameter is hidden (optional for set, add) |
| `operation` | enum | yes | Operation to perform: list (all parameters), get (single parameter), set (update value), add (new parameter), delete (remove parameter) (one of: list, get, set, add, delete) |
| `param_type` | integer | no | Parameter type (0=String, 1=Boolean, 2=Integer, 3=Float) (optional for set, add). Default: 0 (range: 0-3) |
| `parameter_name` | string | no | Name of the parameter (required for get, set, add, delete); matched without regard to case, as Altium treats parameter names |
| `read_only_state` | integer | no | Read-only state (0=editable, 1=read-only) (optional for set, add). Default: 0 (range: 0-1) |
| `unique_id` | string | no | 8-char Altium unique ID (optional for set, add). Default: auto-generated |
| `value` | string | no | Parameter value (required for set, add) |
| `x` | integer | no | X position in schematic units (optional for set, add) |
| `y` | integer | no | Y position in schematic units (optional for set, add) |

## `manage_schlib_footprints`

**Manage symbol footprint links** — writes files (a backup is taken first)

Manage the footprint links (PCB models) of one symbol in a .SchLib: list them, add one, or remove one. A link names a .PcbLib footprint; add refuses a name already linked
and remove refuses one that is not (names match without regard to case), and neither touches the .PcbLib itself. Give library_path on add when Altium should resolve and
preview the footprint from a specific library; omit it to link by name only. Every change backs the library up to a timestamped .bak beside it (the five newest are kept)
before saving. Use manage_schlib_parameters for parameters such as Value, get_component or read_schlib to see the links together with the rest of the symbol, and
update_component to rewrite a symbol wholesale.

**Example**

```json
{
    "arguments": {
        "component_name": "LM358",
        "filepath": "./MyLibrary.SchLib",
        "footprint_name": "SOIC-8_3.9x4.9mm",
        "operation": "add"
    },
    "name": "manage_schlib_footprints"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_name` | string | yes | Name of the symbol to manage footprints for |
| `description` | string | no | Footprint description (optional for add). Keep to 256 characters if the library will be imported into an Altium 365 workspace — that importer refuses longer ones; a longer description is written and reported as a validation warning. |
| `filepath` | string | yes | Path to the Altium SchLib file |
| `footprint_name` | string | no | Footprint name (required for add, remove) |
| `library_path` | string | no | Optional (add): absolute path to the .PcbLib containing the footprint, written as ModelDatafile0 so Altium can resolve and preview the model. Omit to link by name only (requires the library to be installed/in the project, else 'footprint not found'). |
| `operation` | enum | yes | Operation to perform: list (all footprints), add (new footprint link), remove (delete footprint link) (one of: list, add, remove) |

## `compare_components`

**Compare two components** — read-only

Compare two named components — from one library or two of the same type (.PcbLib with .PcbLib, .SchLib with .SchLib) — and report every difference: description and
parameters, then per primitive kind: pads, vias, tracks, arcs, regions, text, fills and 3D bodies of a footprint; pins, every graphic shape, parameters and footprint
links of a symbol. Geometry compares within tolerance (mm, default 0.001) and unmatched items are reported per side; include_geometry false compares counts and properties
only. Identity (GUIDs, unique ids) is never a difference. Read-only; returns identical, difference_count, the differences and per-kind count summaries. Use diff_libraries
first to find which components differ between two whole libraries, then this tool on one pair; use get_component for one component's full data rather than a comparison.

**Example**

```json
{
    "arguments": {
        "component_a": "RESC0603_V1",
        "component_b": "RESC0603_V2",
        "filepath_a": "./LibraryA.PcbLib",
        "filepath_b": "./LibraryB.PcbLib",
        "include_geometry": true,
        "tolerance": 0.001
    },
    "name": "compare_components"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_a` | string | yes | Name of the first component |
| `component_b` | string | yes | Name of the second component |
| `filepath_a` | string | yes | Path to the first library file (.PcbLib or .SchLib) |
| `filepath_b` | string | yes | Path to the second library file (can be same as filepath_a) |
| `include_geometry` | boolean | no | Include detailed geometry comparisons for primitives (default: true) |
| `tolerance` | number | no | Tolerance for floating-point comparisons in mm (default: 0.001) |

## `repair_library`

**Repair a library** — writes files (a backup is taken first)

Repair a PcbLib by removing orphaned 3D-model data: (1) embedded models not referenced by any footprint, and (2) component body references that point to non-existent
models. This fixes libraries where STEP model data is missing but references remain (validate_library reports both conditions). PcbLib only; a SchLib is refused.

**Example**

```json
{
    "arguments": {
        "dry_run": true,
        "filepath": "./MyLibrary.PcbLib"
    },
    "name": "repair_library"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `dry_run` | boolean | no | If true, report what would be fixed without making changes (default: false) |
| `filepath` | string | yes | Path to the library file (.PcbLib) |

## `list_backups`

**List backups** — read-only

List available backup files for an Altium library. Shows timestamped .bak files that were automatically created before write operations.

**Example**

```json
{
    "arguments": {
        "filepath": "./MyLibrary.PcbLib"
    },
    "name": "list_backups"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `filepath` | string | yes | Path to the library file (.PcbLib or .SchLib) |

## `restore_backup`

**Restore a backup** — writes files (a backup is taken first)

Restore an Altium library file from a backup. If no specific backup is specified, restores from the most recent backup. The current file is snapshotted as a new backup
first (reported as pre_restore_backup), so a wrong pick is itself reversible, and the restore is written atomically.

**Example**

```json
{
    "arguments": {
        "backup_path": "MyLibrary.PcbLib.20260125_091500.bak",
        "filepath": "./MyLibrary.PcbLib"
    },
    "name": "restore_backup"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `backup_path` | string | no | Optional: specific backup file to restore from. If not provided, uses most recent backup. |
| `filepath` | string | yes | Path to the library file to restore |

## `bulk_rename`

**Rename components by pattern** — writes files (a backup is taken first)

Rename multiple components in a library using regex pattern matching. Supports capture groups for flexible renaming (e.g., 'RESC(.*)' -> 'RES_$1').

**Example**

```json
{
    "arguments": {
        "dry_run": true,
        "filepath": "./MyLibrary.PcbLib",
        "pattern": "^RESC(.*)$",
        "replacement": "RES_$1"
    },
    "name": "bulk_rename"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `dry_run` | boolean | no | If true, show what would be renamed without making changes (default: false) |
| `filepath` | string | yes | Path to the library file (.PcbLib or .SchLib) |
| `pattern` | string | yes | Regex pattern to match component names (e.g., '^RESC(.*)$') |
| `replacement` | string | yes | Replacement string with optional capture groups (e.g., 'RES_$1') |

## `update_pad`

**Update a pad** — writes files (a backup is taken first)

Update specific properties of a pad in a PcbLib footprint without replacing the entire component. Find pad by designator and apply only the specified updates. On a
stacked pad (stack_mode other than simple) a width/height/shape change also reaches the per-layer tables: layers that shared the old primary value follow it, layers with
their own value keep it, and the response reports how many followed.

**Example**

```json
{
    "arguments": {
        "component_name": "RESC0603",
        "designator": "1",
        "dry_run": false,
        "filepath": "./MyLibrary.PcbLib",
        "updates": {
            "height": 0.9,
            "shape": "rectangle",
            "width": 1.0
        }
    },
    "name": "update_pad"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_name` | string | yes | Name of the footprint containing the pad |
| `designator` | string | yes | Pad designator (e.g., '1', '2', 'A1') |
| `dry_run` | boolean | no | If true, show what would change without saving (default: false) |
| `filepath` | string | yes | Path to the .PcbLib file |
| `updates` | object | yes | Properties to update (only specified properties are changed) |

## `update_primitive`

**Update a primitive** — writes files (a backup is taken first)

Update specific properties of a primitive (track, arc, text, fill, region or via) in a PcbLib footprint. Find the primitive by type and index (its position in
read_pcblib's list for that type), apply only the specified updates. Moving a primitive to another layer drops the layer carriers it was read with (a region's v7_layer,
an unmapped raw_layer_id) so the new layer's own token is written. On a stacked via (diameter_stack_mode other than simple) a diameter change also reaches
per_layer_diameters: layers that shared the old diameter follow it, layers with their own value keep it, and the response reports how many followed.

**Example**

```json
{
    "arguments": {
        "component_name": "RESC0603",
        "dry_run": false,
        "filepath": "./MyLibrary.PcbLib",
        "index": 0,
        "primitive_type": "track",
        "updates": {
            "layer": "Top Overlay",
            "width": 0.15
        }
    },
    "name": "update_primitive"
}
```

**Parameters**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `component_name` | string | yes | Name of the footprint containing the primitive |
| `dry_run` | boolean | no | If true, show what would change without saving (default: false) |
| `filepath` | string | yes | Path to the .PcbLib file |
| `index` | integer | yes | Zero-based index of the primitive within its type array (min: 0) |
| `primitive_type` | enum | yes | Type of primitive to update. Addressed by `index` into that primitive list. Pads are not here — they have a designator, so use update_pad. (one of: track, arc, region, text, fill, via) |
| `updates` | object | yes | Properties to update (only specified properties are changed). Valid properties depend on primitive_type — track: x1, y1, x2, y2, width, layer; arc: x/x1, y/y1, radius, start_angle, end_angle, width, layer; text: x, y, height, rotation, text, layer; fill: x/x1, y/y1, x2, y2, rotation, layer; region: layer; via: x, y, diameter, hole_size, from_layer, to_layer. Any other key is refused. |
