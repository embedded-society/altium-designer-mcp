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

---

## Primitive Types

### Footprint Primitives (PcbLib)

| Primitive | Description |
|-----------|-------------|
| **Pad** | SMD or through-hole pad with designator, position, size, shape, layer |
| **Via** | Vertical interconnect with layer span, hole size, and thermal relief |
| **Track** | Line segment on any layer (silkscreen, assembly, etc.) |
| **Arc** | Arc or circle on any layer |
| **Region** | Filled polygon (courtyard, copper pour) |
| **Text** | Text string with font, size, position, layer |
| **Fill** | Filled rectangle on any layer |
| **ComponentBody** | 3D model reference (embedded STEP models) |

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
