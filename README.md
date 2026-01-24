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
        "offset": 0
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `component_name` | Fetch only this specific footprint |
| `limit` | Maximum footprints to return |
| `offset` | Skip first N footprints |

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

### `list_components`

List component names in an Altium library file.

```json
{
    "name": "list_components",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib"
    }
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

```json
{
    "name": "delete_component",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "component_names": ["OLD_FOOTPRINT", "UNUSED_COMPONENT"]
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `component_names` | Array of component names to delete |

Returns per-component status (`deleted` or `not_found`) and updated component counts.

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
        "format": "json"
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `format` | Export format: `json` for full data, `csv` for summary table |

**JSON format** returns complete component data including all primitives.

**CSV format** returns a summary table with columns: name, description, pad/pin count, etc.

### `extract_step_model`

Extract embedded STEP 3D models from an Altium .PcbLib file. Models are stored compressed inside the library; this tool extracts them to standalone .step files.

```json
{
    "name": "extract_step_model",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "output_path": "./extracted_model.step",
        "model": "RESC1005X04L.step"
    }
}
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `filepath` | Yes | Path to the .PcbLib file containing embedded 3D models |
| `output_path` | No | Path where the extracted .step file will be saved. If omitted, returns base64-encoded data. |
| `model` | No | Model name (e.g., `RESC1005X04L.step`) or GUID to extract. If omitted and only one model exists, extracts it automatically. If multiple models exist and no model specified, lists available models. |

**Response (listing models):**

```json
{
    "status": "list",
    "filepath": "./MyLibrary.PcbLib",
    "message": "Multiple models found. Specify 'model' parameter with name or ID to extract.",
    "model_count": 3,
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

Returns ASCII art with legend: `#` = pad, `-` = track, `o` = arc, `+` = origin.

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

Returns ASCII art with legend: `1-9/*` = pin, `|-+` = rectangle, `~` = pin line, `o` = arc, `O` = ellipse, `+` = origin.

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
| Round | `"round"` | Circular pads, default for through-hole |
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
| **Label** | Text label |
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

For parametric 3D model generation, a dedicated mechanical MCP server is planned as a future project.

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

## Licence

Copyright (C) 2026 The Embedded Society <https://github.com/embedded-society/altium-designer-mcp>.

GNU General Public License v3.0 — see [LICENCE](LICENCE).

---

## Links

- [MCP Specification](https://modelcontextprotocol.io/)
- [Report an Issue](https://github.com/embedded-society/altium-designer-mcp/issues)

---

## Sample Files

Sample Altium library files are included in the `scripts/` folder for testing and development.

See [scripts/README.md](scripts/README.md) for details on available sample files and analysis scripts.

---

## Prior Art

This project builds on the work of:

- [AltiumSharp](https://github.com/issus/AltiumSharp) — C# Altium file parser (MIT)
- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) — Python Altium library
- [python-altium](https://github.com/vadmium/python-altium) — Altium format documentation
