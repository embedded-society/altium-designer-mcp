# AI Workflow Guide

This document explains how an AI assistant uses altium-designer-mcp to create Altium components.

## Core Principle

**The AI handles the intelligence. The tool handles file I/O.**

See [VISION.md](VISION.md) for the full responsibility split and architectural rationale.

---

## The Complete Workflow

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                    AI's Component Creation Workflow                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. UNDERSTAND THE REQUEST                                                  │
│     Engineer: "Create a 0603 chip resistor footprint"                       │
│     AI reasons about the component requirements                             │
│                                                                             │
│  2. CALCULATE DIMENSIONS (AI's job)                                         │
│     AI applies IPC-7351B formulas:                                          │
│     • Body: 1.6mm × 0.8mm                                                   │
│     • Terminal length: 0.3mm                                                │
│     • Pad size = terminal + toe + heel                                      │
│     • Courtyard = body + margins                                            │
│                                                                             │
│  3. DEFINE PRIMITIVES (AI's job)                                            │
│     AI constructs the complete footprint:                                   │
│     • Pads with exact positions and sizes                                   │
│     • Silkscreen tracks                                                     │
│     • Courtyard region                                                      │
│     • Assembly outline                                                      │
│                                                                             │
│  4. WRITE TO FILE (Tool's job)                                              │
│     AI calls: write_pcblib { filepath, footprints }                         │
│     Tool writes the OLE compound document                                   │
│                                                                             │
│  5. VERIFY                                                                  │
│     AI calls: read_pcblib to verify the result                              │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Step-by-Step Example: Creating a 0603 Resistor

### 1. AI Calculates the Footprint

The AI applies IPC-7351B knowledge:

```text
Component: 0603 Chip Resistor (1608 metric)
Body: 1.6mm × 0.8mm × 0.55mm
Terminal: 0.3mm

IPC-7351B Calculations (Nominal density):
- Toe extension: 0.35mm
- Heel extension: 0.35mm
- Side extension: 0.05mm

Pad dimensions (using manufacturer-recommended values):
- Width: 0.9mm (terminal + extensions, adjusted for process)
- Height: 0.95mm (body height + side extensions, adjusted)
- Centre-to-centre span: 1.5mm

IPC Name: RESC1608X55N
```

### 2. AI Constructs Primitives

```json
{
    "name": "RESC1608X55N",
    "description": "Chip resistor, 0603 (1608 metric), IPC-7351B Nominal",
    "pads": [
        {
            "designator": "1",
            "x": -0.75,
            "y": 0,
            "width": 0.9,
            "height": 0.95,
            "shape": "rounded_rectangle",
            "layer": "Top Layer"
        },
        {
            "designator": "2",
            "x": 0.75,
            "y": 0,
            "width": 0.9,
            "height": 0.95,
            "shape": "rounded_rectangle",
            "layer": "Top Layer"
        }
    ],
    "tracks": [
        { "x1": -0.8, "y1": -0.55, "x2": 0.8, "y2": -0.55, "width": 0.12, "layer": "Top Overlay" },
        { "x1": -0.8, "y1": 0.55, "x2": 0.8, "y2": 0.55, "width": 0.12, "layer": "Top Overlay" }
    ],
    "regions": [
        {
            "vertices": [
                { "x": -1.45, "y": -0.73 },
                { "x": 1.45, "y": -0.73 },
                { "x": 1.45, "y": 0.73 },
                { "x": -1.45, "y": 0.73 }
            ],
            "layer": "Top Courtyard"
        }
    ]
}
```

### 3. AI Calls write_pcblib

**MCP Tool Call:**

```json
{
    "name": "write_pcblib",
    "arguments": {
        "filepath": "./Passives.PcbLib",
        "footprints": [
            {
                "name": "RESC1608X55N",
                "description": "Chip resistor, 0603 (1608 metric)",
                "pads": [...],
                "tracks": [...],
                "regions": [...]
            }
        ]
    }
}
```

**Response:**

```json
{
    "success": true,
    "footprints_written": 1
}
```

### 4. AI Verifies the Result

**MCP Tool Call:**

```json
{
    "name": "read_pcblib",
    "arguments": {
        "filepath": "./Passives.PcbLib"
    }
}
```

---

## Primitive Types

The AI provides complete primitive definitions. The tool writes them.

### Pad

```json
{
    "designator": "1",
    "x": 0,
    "y": 0,
    "width": 1.0,
    "height": 0.8,
    "shape": "rounded_rectangle",
    "layer": "Top Layer",
    "hole_size": null,
    "rotation": 0
}
```

### Via

```json
{
    "x": 0,
    "y": 0,
    "diameter": 0.6,
    "hole_size": 0.3,
    "from_layer": "Top Layer",
    "to_layer": "Bottom Layer"
}
```

### Track

```json
{
    "x1": -1.0,
    "y1": 0.5,
    "x2": 1.0,
    "y2": 0.5,
    "width": 0.12,
    "layer": "Top Overlay"
}
```

### Arc

```json
{
    "x": 0,
    "y": 0,
    "radius": 0.5,
    "start_angle": 0,
    "end_angle": 360,
    "width": 0.12,
    "layer": "Top Overlay"
}
```

### Region

```json
{
    "vertices": [
        { "x": -1, "y": -1 },
        { "x": 1, "y": -1 },
        { "x": 1, "y": 1 },
        { "x": -1, "y": 1 }
    ],
    "layer": "Top Courtyard"
}
```

### Text

```json
{
    "x": 0,
    "y": 1.5,
    "text": ".Designator",
    "height": 0.8,
    "layer": "Top Overlay",
    "rotation": 0
}
```

### Fill

```json
{
    "x1": -0.8,
    "y1": -0.4,
    "x2": 0.8,
    "y2": 0.4,
    "layer": "Top Assembly",
    "rotation": 0
}
```

### ComponentBody

```json
{
    "model_id": "GUID-HERE",
    "model_name": "RESC1608X55.step",
    "embedded": true,
    "rotation_x": 0,
    "rotation_y": 0,
    "rotation_z": 0,
    "z_offset": 0,
    "overall_height": 0.55,
    "standoff_height": 0,
    "layer": "Top 3D Body"
}
```

### step_model (3D Model Attachment)

To attach a STEP 3D model to a footprint, include the `step_model` property:

```json
{
    "name": "RESC1608X55N",
    "description": "Chip resistor with 3D model",
    "pads": [...],
    "step_model": {
        "filepath": "./models/RESC1608X55.step",
        "x_offset": 0,
        "y_offset": 0,
        "z_offset": 0,
        "rotation": 0
    }
}
```

| Property | Description |
|----------|-------------|
| `filepath` | Path to the .step file (will be embedded in the library) |
| `x_offset` | X offset in mm (default: 0) |
| `y_offset` | Y offset in mm (default: 0) |
| `z_offset` | Z offset in mm (default: 0) |
| `rotation` | Z rotation in degrees (default: 0) |

The STEP file is read from disk and embedded in the PcbLib file during write.

### Extracting Embedded STEP Models

To extract embedded STEP models from a PcbLib, use `extract_step_model`:

```json
{
    "name": "extract_step_model",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "output_path": "./extracted_model.step",
        "model": "RESC1608X55.step"
    }
}
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `filepath` | Yes | Path to the PcbLib containing embedded models |
| `output_path` | No | Path for extracted .step file (if omitted, returns base64) |
| `model` | No | Model name or GUID (if omitted, lists available models) |

This is useful for:

- Inspecting embedded 3D models
- Reusing models across libraries
- Backing up 3D model data

---

## Standard Altium Layers

See [README.md § Standard Altium Layers](../README.md#standard-altium-layers) for the layer reference.

---

## Working with Large Libraries

For libraries with many components, use pagination to avoid output limits:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ LARGE LIBRARY WORKFLOW                                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. LIST COMPONENTS FIRST                                                   │
│     AI calls: list_components { filepath }                                  │
│     Returns: ["RESC0402...", "RESC0603...", ...]                            │
│                                                                             │
│  2. FETCH SPECIFIC COMPONENTS                                               │
│     AI calls: read_pcblib { filepath, component_name: "RESC0603..." }       │
│     Returns: Single footprint with full details                             │
│                                                                             │
│  3. OR PAGINATE THROUGH ALL                                                 │
│     AI calls: read_pcblib { filepath, limit: 5, offset: 0 }                 │
│     Response includes: has_more: true, total_count: 50                      │
│     AI calls: read_pcblib { filepath, limit: 5, offset: 5 }                 │
│     ... continues until has_more: false                                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Batch Component Creation

The AI can create entire libraries efficiently:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ BATCH CREATION: 100 components in minutes                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  User: "Create a resistor library with all standard chip sizes"             │
│                                                                             │
│  AI:                                                                        │
│    1. List standard chip sizes: [0201, 0402, 0603, 0805, 1206, 2512]        │
│    2. For each size:                                                        │
│       - Look up body dimensions                                             │
│       - Apply IPC-7351B formulas                                            │
│       - Construct pad, track, region primitives                             │
│    3. Call write_pcblib with all footprints                                 │
│    4. Verify with read_pcblib                                               │
│                                                                             │
│  Result: Complete resistor library created                                  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Library Management

### Deleting Components

Components can be removed from existing libraries using `delete_component`:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ COMPONENT DELETION WORKFLOW                                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. LIST EXISTING COMPONENTS (optional)                                     │
│     AI calls: list_components { filepath }                                  │
│     Returns: ["RESC0402...", "RESC0603...", "OLD_FOOTPRINT", ...]           │
│                                                                             │
│  2. DELETE UNWANTED COMPONENTS                                              │
│     AI calls: delete_component {                                            │
│         filepath,                                                           │
│         component_names: ["OLD_FOOTPRINT", "DEPRECATED_0402"]               │
│     }                                                                       │
│     Returns: per-component status (deleted/not_found)                       │
│                                                                             │
│  3. VERIFY (optional)                                                       │
│     AI calls: list_components { filepath }                                  │
│     Confirm components were removed                                         │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Example Response:**

```json
{
    "status": "success",
    "filepath": "./Passives.PcbLib",
    "file_type": "PcbLib",
    "original_count": 10,
    "deleted_count": 2,
    "remaining_count": 8,
    "results": [
        { "name": "OLD_FOOTPRINT", "status": "deleted" },
        { "name": "DEPRECATED_0402", "status": "deleted" }
    ]
}
```

### Renaming Components

Use `rename_component` for atomic component renaming:

```json
{
    "name": "rename_component",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "old_name": "OLD_NAME",
        "new_name": "NEW_NAME"
    }
}
```

This is more efficient than the manual copy + delete approach and preserves all primitives
and properties in a single atomic operation.

### Validating Libraries

Use `validate_library` to check for common issues before using a library:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ LIBRARY VALIDATION WORKFLOW                                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. VALIDATE THE LIBRARY                                                    │
│     AI calls: validate_library { filepath }                                 │
│     Returns: status (valid/warnings/invalid) and list of issues             │
│                                                                             │
│  2. REVIEW ISSUES                                                           │
│     • Errors: Must be fixed (duplicate designators, invalid dimensions)     │
│     • Warnings: Should be reviewed (empty components, missing graphics)     │
│                                                                             │
│  3. FIX ISSUES (if needed)                                                  │
│     • Delete problematic components                                         │
│     • Re-create with correct parameters                                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Example Response (with issues):**

```json
{
    "status": "warnings",
    "filepath": "./MyLibrary.PcbLib",
    "file_type": "PcbLib",
    "component_count": 5,
    "error_count": 0,
    "warning_count": 2,
    "issues": [
        { "severity": "warning", "component": "TEST_FP", "issue": "Footprint has no pads" },
        { "severity": "warning", "component": "OLD_RES", "issue": "Footprint has no pads" }
    ]
}
```

### Exporting Libraries

Use `export_library` to export library contents for version control or external processing:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ LIBRARY EXPORT WORKFLOW                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  JSON EXPORT (full data)                                                    │
│  AI calls: export_library { filepath, format: "json" }                      │
│  Returns: Complete component data with all primitives                       │
│  Use for: Backup, version control diffs, data migration                     │
│                                                                             │
│  CSV EXPORT (summary table)                                                 │
│  AI calls: export_library { filepath, format: "csv" }                       │
│  Returns: Summary table (name, description, counts)                         │
│  Use for: Inventory, documentation, spreadsheet import                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Example CSV Output (PcbLib):**

```csv
name,description,pad_count,track_count,arc_count,region_count,text_count,has_3d_model
RESC0603,Chip resistor 0603,2,4,0,1,2,no
CAPC0402,Chip capacitor 0402,2,4,0,1,2,no
```

### Comparing Libraries

Use `diff_libraries` to compare two versions of a library:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ LIBRARY DIFF WORKFLOW                                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  COMPARE TWO LIBRARY VERSIONS                                               │
│  AI calls: diff_libraries {                                                 │
│      filepath_a: "./OldLibrary.PcbLib",                                     │
│      filepath_b: "./NewLibrary.PcbLib"                                      │
│  }                                                                          │
│                                                                             │
│  Returns:                                                                   │
│  • added: Components in new library only                                    │
│  • removed: Components in old library only                                  │
│  • modified: Components with changes (counts, descriptions)                 │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Example Response:**

```json
{
    "status": "success",
    "file_type": "PcbLib",
    "summary": {
        "components_in_a": 10,
        "components_in_b": 12,
        "added_count": 3,
        "removed_count": 1,
        "modified_count": 2,
        "unchanged_count": 7
    },
    "added": ["RESC0201", "CAPC0201", "INDC0402"],
    "removed": ["OLD_FOOTPRINT"],
    "modified": [
        { "name": "RESC0603", "changes": ["pad_count: 2 -> 4"] },
        { "name": "CAPC0805", "changes": ["description: '' -> 'Updated'"] }
    ]
}
```

### Batch Operations

Use `batch_update` to perform library-wide updates efficiently:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ BATCH UPDATE WORKFLOW                                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  UPDATE TRACK WIDTHS ACROSS ALL FOOTPRINTS                                  │
│  AI calls: batch_update {                                                   │
│      filepath: "./MyLibrary.PcbLib",                                        │
│      operation: "update_track_width",                                       │
│      parameters: {                                                          │
│          from_width: 0.2,                                                   │
│          to_width: 0.25,                                                    │
│          tolerance: 0.001                                                   │
│      }                                                                      │
│  }                                                                          │
│                                                                             │
│  RENAME LAYERS ACROSS ALL FOOTPRINTS                                        │
│  AI calls: batch_update {                                                   │
│      filepath: "./MyLibrary.PcbLib",                                        │
│      operation: "rename_layer",                                             │
│      parameters: {                                                          │
│          from_layer: "Mechanical 1",                                        │
│          to_layer: "Mechanical 2"                                           │
│      }                                                                      │
│  }                                                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Example Response (track width update):**

```json
{
    "status": "success",
    "operation": "update_track_width",
    "from_width": 0.2,
    "to_width": 0.25,
    "tolerance": 0.001,
    "total_tracks_updated": 48,
    "footprints_updated_count": 12,
    "footprints_updated": [
        { "name": "RESC0603", "tracks_updated": 4 },
        { "name": "CAPC0805", "tracks_updated": 6 }
    ]
}
```

**PcbLib operations:**

| Operation | Description |
|-----------|-------------|
| `update_track_width` | Change track widths matching a value (with tolerance) |
| `rename_layer` | Move primitives from one layer to another |

**SchLib operations:**

| Operation | Description |
|-----------|-------------|
| `update_parameters` | Set parameter values across multiple symbols |

**Example: Update parameters across symbols (SchLib)**

```json
{
    "name": "batch_update",
    "arguments": {
        "filepath": "./MyLibrary.SchLib",
        "operation": "update_parameters",
        "parameters": {
            "param_name": "Manufacturer",
            "param_value": "Texas Instruments",
            "symbol_filter": "^LM.*",
            "add_if_missing": true
        }
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `param_name` | Name of the parameter to update |
| `param_value` | New value to set |
| `symbol_filter` | Optional regex to filter which symbols to update |
| `add_if_missing` | If true, add the parameter to symbols that don't have it (default: false) |

### Copying Components

Use `copy_component` to duplicate components for creating variants:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ COMPONENT COPY WORKFLOW                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  DUPLICATE A FOOTPRINT                                                      │
│  AI calls: copy_component {                                                 │
│      filepath: "./MyLibrary.PcbLib",                                        │
│      source_name: "RESC0603_IPC_MEDIUM",                                    │
│      target_name: "RESC0603_IPC_MEDIUM_NARROW",                             │
│      description: "0603 resistor - narrow variant"                          │
│  }                                                                          │
│                                                                             │
│  DUPLICATE A SYMBOL                                                         │
│  AI calls: copy_component {                                                 │
│      filepath: "./MyLibrary.SchLib",                                        │
│      source_name: "Generic_Resistor",                                       │
│      target_name: "Generic_Resistor_2W"                                     │
│  }                                                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Example Response:**

```json
{
    "status": "success",
    "filepath": "./MyLibrary.PcbLib",
    "file_type": "PcbLib",
    "source_name": "RESC0603_IPC_MEDIUM",
    "target_name": "RESC0603_IPC_MEDIUM_NARROW",
    "component_count": 15
}
```

**Common use cases:**

- Create density variants (Medium → Dense, Loose)
- Create thermal variants (standard pads → extended pads)
- Create test variants with exposed pins
- Duplicate symbols for multi-part components

### Copying Components Between Libraries

Use `copy_component_cross_library` to copy components from one library to another:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ CROSS-LIBRARY COPY WORKFLOW                                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  COPY A FOOTPRINT TO ANOTHER LIBRARY                                        │
│  AI calls: copy_component_cross_library {                                   │
│      source_filepath: "./ComponentLib.PcbLib",                              │
│      target_filepath: "./ProjectLib.PcbLib",                                │
│      component_name: "RESC0603_IPC_MEDIUM"                                  │
│  }                                                                          │
│                                                                             │
│  COPY WITH RENAME                                                           │
│  AI calls: copy_component_cross_library {                                   │
│      source_filepath: "./OldLib.PcbLib",                                    │
│      target_filepath: "./NewLib.PcbLib",                                    │
│      component_name: "OLD_FP_NAME",                                         │
│      new_name: "NEW_FP_NAME",                                               │
│      description: "Copied and renamed footprint"                            │
│  }                                                                          │
│                                                                             │
│  COPY SYMBOL TO NEW LIBRARY                                                 │
│  AI calls: copy_component_cross_library {                                   │
│      source_filepath: "./MasterSchLib.SchLib",                              │
│      target_filepath: "./ProjectSchLib.SchLib",                             │
│      component_name: "Generic_Resistor"                                     │
│  }                                                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Example Response:**

```json
{
    "status": "success",
    "source_filepath": "./ComponentLib.PcbLib",
    "target_filepath": "./ProjectLib.PcbLib",
    "file_type": "PcbLib",
    "component_name": "RESC0603_IPC_MEDIUM",
    "target_name": "RESC0603_IPC_MEDIUM",
    "target_component_count": 5,
    "message": "Copied 'RESC0603_IPC_MEDIUM' from './ComponentLib.PcbLib' to './ProjectLib.PcbLib'"
}
```

**Common use cases:**

- Consolidate components from multiple libraries into one
- Share standard components between projects
- Create project-specific libraries from master libraries
- Migrate components during library reorganisation

### Previewing Footprints

Use `render_footprint` to generate an ASCII art visualisation for quick preview:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ FOOTPRINT PREVIEW WORKFLOW                                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  RENDER A FOOTPRINT                                                         │
│  AI calls: render_footprint {                                               │
│      filepath: "./MyLibrary.PcbLib",                                        │
│      component_name: "RESC0603_IPC_MEDIUM",                                 │
│      scale: 2.0,                                                            │
│      max_width: 60                                                          │
│  }                                                                          │
│                                                                             │
│  Returns ASCII art showing:                                                 │
│  • Pads (#) with designator at centre                                       │
│  • Tracks (-) as lines                                                      │
│  • Arcs (o) as circles                                                      │
│  • Origin (+) crosshair                                                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Example Output:**

```text
Footprint: RESC0603 (2.60 x 1.20 mm)
Pads: 2, Tracks: 4, Arcs: 0
--------------------------------------------------------------
|                                                            |
|          ------                ------                      |
|         |  1   |      +       |  2   |                     |
|          ------                ------                      |
|                                                            |
--------------------------------------------------------------
Legend: # = pad, - = track, o = arc, + = origin
```

**Use cases:**

- Verify footprint geometry before creating
- Debug layout issues
- Document footprints in text format

### Previewing Symbols

Use `render_symbol` to generate an ASCII art visualisation of schematic symbols:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ SYMBOL PREVIEW WORKFLOW                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  RENDER A SYMBOL                                                            │
│  AI calls: render_symbol {                                                  │
│      filepath: "./MyLibrary.SchLib",                                        │
│      component_name: "LM358",                                               │
│      scale: 1.0,                                                            │
│      max_width: 60,                                                         │
│      part_id: 1                                                             │
│  }                                                                          │
│                                                                             │
│  Returns ASCII art showing:                                                 │
│  • Pins (1-9/*) with designator                                             │
│  • Rectangles (|-+) as box outlines                                         │
│  • Pin lines (~) extending from body                                        │
│  • Arcs (o) and ellipses (O)                                                │
│  • Origin (+) crosshair                                                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Example Output:**

```text
Symbol: LM358 (Part 1 of 2)
Pins: 4, Rectangles: 1, Lines: 0
--------------------------------------------------------------
|                                                            |
|        +--------+                                          |
|   3 ~~~|        |                                          |
|        |   +    |~~~ 1                                     |
|   2 ~~~|        |                                          |
|        +--------+                                          |
|                                                            |
--------------------------------------------------------------
Legend: 1-9/* = pin, |-+ = rectangle, ~ = pin line, + = origin
```

**Use cases:**

- Verify symbol pin placement before creating
- Check multi-part symbol layouts
- Debug pin orientation issues
- Document symbols in text format

### Managing Symbol Parameters

Use `manage_schlib_parameters` to read and modify component parameters like Value,
Manufacturer, Part Number, etc.:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ PARAMETER MANAGEMENT WORKFLOW                                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  LIST ALL PARAMETERS                                                         │
│  AI calls: manage_schlib_parameters {                                        │
│      filepath: "./MyLibrary.SchLib",                                         │
│      component_name: "LM358",                                                │
│      operation: "list"                                                       │
│  }                                                                          │
│                                                                             │
│  SET A PARAMETER VALUE                                                       │
│  AI calls: manage_schlib_parameters {                                        │
│      filepath: "./MyLibrary.SchLib",                                         │
│      component_name: "LM358",                                                │
│      operation: "set",                                                       │
│      parameter_name: "Value",                                                │
│      value: "LM358D"                                                         │
│  }                                                                          │
│                                                                             │
│  ADD A NEW PARAMETER                                                         │
│  AI calls: manage_schlib_parameters {                                        │
│      filepath: "./MyLibrary.SchLib",                                         │
│      component_name: "LM358",                                                │
│      operation: "add",                                                       │
│      parameter_name: "Manufacturer",                                         │
│      value: "Texas Instruments"                                              │
│  }                                                                          │
│                                                                             │
│  DELETE A PARAMETER                                                          │
│  AI calls: manage_schlib_parameters {                                        │
│      filepath: "./MyLibrary.SchLib",                                         │
│      component_name: "LM358",                                                │
│      operation: "delete",                                                    │
│      parameter_name: "OldParameter"                                          │
│  }                                                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Common parameters:**

| Parameter | Description |
|-----------|-------------|
| `Value` | Component value (e.g., "10k", "100nF") |
| `Manufacturer` | Component manufacturer |
| `Part Number` | Manufacturer part number |
| `Description` | Component description |
| `Datasheet` | Link to datasheet |

**Use cases:**

- Populate component data from datasheets
- Update values across library symbols
- Add manufacturer and part number information
- Clean up unused parameters

### Managing Footprint Links

Use `manage_schlib_footprints` to link schematic symbols to PCB footprints:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ FOOTPRINT LINK MANAGEMENT WORKFLOW                                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  LIST LINKED FOOTPRINTS                                                      │
│  AI calls: manage_schlib_footprints {                                        │
│      filepath: "./MyLibrary.SchLib",                                         │
│      component_name: "LM358",                                                │
│      operation: "list"                                                       │
│  }                                                                          │
│                                                                             │
│  ADD A FOOTPRINT LINK                                                        │
│  AI calls: manage_schlib_footprints {                                        │
│      filepath: "./MyLibrary.SchLib",                                         │
│      component_name: "LM358",                                                │
│      operation: "add",                                                       │
│      footprint_name: "SOIC-8_3.9x4.9mm",                                     │
│      description: "SOIC 8-pin package"                                       │
│  }                                                                          │
│                                                                             │
│  REMOVE A FOOTPRINT LINK                                                     │
│  AI calls: manage_schlib_footprints {                                        │
│      filepath: "./MyLibrary.SchLib",                                         │
│      component_name: "LM358",                                                │
│      operation: "remove",                                                    │
│      footprint_name: "DIP-8_W7.62mm"                                         │
│  }                                                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Use cases:**

- Link symbols to multiple package variants (SOIC, DIP, QFN)
- Update footprint references when libraries change
- Clean up obsolete footprint links

---

## Tips for AI Assistants

### 1. Apply IPC-7351B Correctly

The AI is responsible for correct calculations:

- Use appropriate density level (M/N/L) for the application
- Calculate toe, heel, and side extensions
- Determine courtyard margins
- Generate correct IPC names

### 2. Use Consistent Style

When creating multiple components:

- Use the same silkscreen line width (typically 0.12mm or 0.15mm)
- Use the same courtyard margins
- Place silkscreen outside the pads
- Use consistent layer assignments

### 3. Verify Dimensions

Before calling write_pcblib:

- Body length > terminal length
- Pad width and height > 0
- Courtyard encompasses all pads
- Silkscreen doesn't overlap pads

### 4. Handle Through-Hole Components

For through-hole pads:

- Set `layer` to "Multi-Layer"
- Provide `hole_size` in mm
- Pad size should be hole + annular ring

---

## Error Handling

### File Not Found

```json
{
    "error": "Failed to read file: ./Missing.PcbLib"
}
```

### Invalid Primitive

```json
{
    "error": "Invalid parameter 'width': must be positive"
}
```

### Write Error

```json
{
    "error": "Failed to write file: ./Library.PcbLib"
}
```

---

## IPC Standards Reference

When calculating footprints, the AI should apply these IPC standards:

### Primary Standards

| Standard | Description | Key Content |
|----------|-------------|-------------|
| **IPC-7351B** | Generic Requirements for Surface Mount Design and Land Pattern Standard | Pad dimensions, courtyard, naming conventions |
| **IPC-2221** | Generic Standard on Printed Board Design | Through-hole annular rings, via sizing |
| **IPC-2222** | Sectional Design Standard for Rigid Organic Printed Boards | Layer stackup, design rules |

### IPC-7351B Quick Reference

**Density Levels:**

| Level | Code | Application |
|-------|------|-------------|
| Most (M) | N/A | High-density designs, fine-pitch |
| Nominal (N) | N | Standard manufacturing |
| Least (L) | N/A | Wave soldering, hand assembly |

**Naming Convention:**

```text
RESC1608X55N
│   │    │ └── Density: N=Nominal, M=Most, L=Least
│   │    └──── Height in 0.01mm (55 = 0.55mm)
│   └───────── Body size in 0.01mm (1608 = 1.6mm x 0.8mm)
└───────────── Package type (RESC = Chip Resistor)
```

**Common Package Codes:**

| Code | Package Type |
|------|-------------|
| RESC | Chip Resistor |
| CAPC | Chip Capacitor |
| INDC | Chip Inductor |
| DIOM | Molded Diode |
| LEDC | Chip LED |
| SOIC | Small Outline IC |
| QFP | Quad Flat Package |
| QFN | Quad Flat No-Lead |
| BGA | Ball Grid Array |
| SOT | Small Outline Transistor |

### Official IPC Resources

- **IPC Standards Store**: [shop.ipc.org](https://shop.ipc.org/)
- **IPC-7351B LP Calculator**: [PCB Libraries Calculator](https://www.pcblibraries.com/Products/FPL_702-IPC7351LandPatternCalculator.asp)
- **IPC-7351B Naming and Land Pattern Tool**: [PCB Libraries](https://www.pcblibraries.com/)

### Additional References

- **JEDEC Package Outlines**: [jedec.org/standards-documents](https://www.jedec.org/standards-documents)
- **EIA/JEDEC Component Sizes**: Standard chip sizes (0201, 0402, 0603, etc.)
