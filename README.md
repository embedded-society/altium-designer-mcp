# altium-designer-mcp

**An AI-operated Altium Designer libraries editor.**

An MCP server that provides file I/O and primitive placement tools, enabling AI assistants
(Claude Code, Claude Desktop, VSCode Copilot) to create and manage Altium Designer
component libraries.

---

## The Core Idea

**The AI handles the intelligence. The tool handles the file I/O.**

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

---

## How It Works

```
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
│    │                         │  • Silkscreen style          │               │
│    │                         │                              │               │
│    │                         │  write_pcblib(primitives)    │               │
│    │                         ├─────────────────────────────►│               │
│    │                         │                              │ Writes        │
│    │                         │                              │ .PcbLib file  │
│    │                         │◄─────────────────────────────┤               │
│    │                         │  { success: true }           │               │
│    │                         │                              │               │
│    │  "Done! Footprint       │                              │               │
│    │   RESC1608X55N created" │                              │               │
│    │◄────────────────────────┤                              │               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Current Status

**In Development** — MCP infrastructure functional, PcbLib reader/writer implemented.

| Feature | Status |
|---------|--------|
| MCP server with stdio transport | Working |
| Tool definitions and JSON schemas | Working |
| OLE compound document structure | Working |
| Primitive types (Pad, Track, Arc, etc.) | Working |
| PcbLib binary format parsing | Working |
| PcbLib binary format encoding | Working |
| SchLib support | Not yet implemented |

---

## MCP Tools

### `read_pcblib`

Read footprints from an Altium `.PcbLib` file.

```json
{
  "name": "read_pcblib",
  "arguments": {
    "filepath": "./MyLibrary.PcbLib"
  }
}
```

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
        { "vertices": [{"x": -1.45, "y": -0.73}, {"x": 1.45, "y": -0.73}, {"x": 1.45, "y": 0.73}, {"x": -1.45, "y": 0.73}], "layer": "Mechanical 15" }
      ]
    }]
  }
}
```

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

---

## Primitive Types

### Footprint Primitives

| Primitive | Description |
|-----------|-------------|
| **Pad** | SMD or through-hole pad with designator, position, size, shape, layer |
| **Track** | Line segment on any layer (silkscreen, assembly, etc.) |
| **Arc** | Arc or circle on any layer |
| **Region** | Filled polygon (courtyard, copper pour) |
| **Text** | Text string with font, size, position, layer |

### Standard Altium Layers

| Layer | Usage |
|-------|-------|
| Top Layer | Copper pads |
| Multi-Layer | Through-hole pads |
| Top Overlay | Silkscreen |
| Top Paste | Solder paste |
| Top Solder | Solder mask |
| Mechanical 1 | Assembly outline |
| Mechanical 13 | 3D body outline |
| Mechanical 15 | Courtyard |

---

## Installation

### Prerequisites

- Rust 1.75+ (for building from source)

### From Source

```bash
git clone https://github.com/embedded-society/altium-designer-mcp.git
cd altium-designer-mcp
cargo build --release
```

The binary will be at `target/release/altium-designer-mcp`.

### Usage with Claude Desktop

Add to your Claude Desktop MCP configuration:

```json
{
  "mcpServers": {
    "altium": {
      "command": "altium-designer-mcp",
      "args": ["/path/to/libraries"]
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
  "library_path": "/path/to/libraries",
  "logging": {
    "level": "warn"
  }
}
```

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

Copyright (C) 2025 Matej Gomboc <https://github.com/MatejGomboc/altium-designer-mcp>.

GNU General Public License v3.0 — see [LICENCE](LICENCE).

---

## Links

- [MCP Specification](https://modelcontextprotocol.io/)
- [Report an Issue](https://github.com/embedded-society/altium-designer-mcp/issues)

## Sample Files

Sample Altium library files are included in the `scripts/` folder for testing and development:

| File | Description |
|------|-------------|
| `scripts/sample.PcbLib` | Sample PCB footprint library (chip resistors) |
| `scripts/sample.SchLib` | Corresponding schematic symbol library |

These files can be used with the analysis scripts:

```bash
# Python analysis (requires pyaltiumlib)
cd scripts
python analyze_pcblib.py sample.PcbLib

# Rust analysis
cargo test --test pcblib_analysis -- --ignored --nocapture
```

---

## Prior Art

This project builds on the work of:

- [AltiumSharp](https://github.com/issus/AltiumSharp) — C# Altium file parser (MIT)
- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) — Python Altium library
- [python-altium](https://github.com/vadmium/python-altium) — Altium format documentation
