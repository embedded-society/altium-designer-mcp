# altium-designer-mcp

MCP server for AI-assisted Altium Designer component library management with IPC-7351B compliance.

## Overview

**altium-designer-mcp** is a Rust implementation of a [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) server that enables AI assistants to create, read, and manage Altium Designer component libraries.

### The Problem

Creating IPC-compliant Altium components manually is time-consuming:

| Components | Manual Time | With This Tool |
|------------|-------------|----------------|
| 100        | ~2.5 weeks  | ~10 minutes    |
| 500        | ~3 months   | ~1 hour        |
| 1000       | ~6 months   | ~2 hours       |

Each component requires:
- Hunting through datasheets for package dimensions
- Looking up IPC-7351B tables and calculating land patterns
- Manually placing pads, silkscreen, courtyard in Altium
- Creating matching schematic symbol
- Entering 40+ parameter fields

### The Solution

A single Rust binary that:
- Calculates IPC-7351B compliant footprints from package dimensions
- Generates complete components (footprint + symbol + parameters)
- Reads and writes native Altium files (.PcbLib, .SchLib, .DbLib)
- Integrates with Claude Code/Desktop/VSCode via MCP

## Current Status

**Early Development** - The MCP server infrastructure is functional, but Altium file I/O and full IPC calculations are not yet implemented.

Working:
- MCP server with stdio transport
- Tool discovery and invocation
- Placeholder IPC-7351B tools

Not yet implemented:
- Altium binary file reading/writing
- Complete IPC-7351B calculations
- Style extraction
- Symbol generation

## Installation

### From Source

```bash
git clone https://github.com/embedded-society/altium-designer-mcp.git
cd altium-designer-mcp
cargo build --release
```

The binary will be at `target/release/altium-designer-mcp`.

## Usage

### Claude Code CLI

```bash
claude mcp add --scope user --transport stdio altium -- altium-designer-mcp /path/to/libraries
```

### Project Configuration

Add `.mcp.json` to your component library repo:

```json
{
  "mcpServers": {
    "altium": {
      "type": "stdio",
      "command": "altium-designer-mcp",
      "args": ["./libraries"]
    }
  }
}
```

### Command Line

```bash
# With library path
altium-designer-mcp /path/to/libraries

# With config file
altium-designer-mcp --config config.json

# Verbose logging
altium-designer-mcp -vv /path/to/libraries
```

## Available Tools

| Tool                  | Description                              | Status      |
|-----------------------|------------------------------------------|-------------|
| `list_package_types`  | List supported IPC-7351B package types   | Working     |
| `calculate_footprint` | Calculate land pattern from dimensions   | Placeholder |
| `get_ipc_name`        | Generate IPC-7351B compliant name        | Placeholder |

More tools planned: `read_pcblib`, `write_pcblib`, `create_component`, `extract_style_guide`, etc.

## Configuration

Create `~/.altium-designer-mcp/config.json`:

```json
{
  "library_path": "/path/to/default/libraries",
  "ipc": {
    "default_density": "N",
    "thermal_vias": true,
    "courtyard_margin": 0.25
  },
  "style": {
    "silkscreen_line_width": 0.15,
    "assembly_line_width": 0.10,
    "pin1_marker_style": "dot"
  },
  "logging": {
    "level": "warn"
  }
}
```

### Configuration Options

| Section   | Option                    | Description                                    |
|-----------|---------------------------|------------------------------------------------|
| `ipc`     | `default_density`         | M (Most), N (Nominal), L (Least). Default: N   |
| `ipc`     | `thermal_vias`            | Add vias to QFN thermal pads. Default: true    |
| `ipc`     | `courtyard_margin`        | Margin around component in mm. Default: 0.25   |
| `style`   | `silkscreen_line_width`   | Silkscreen line width in mm. Default: 0.15     |
| `style`   | `assembly_line_width`     | Assembly drawing width in mm. Default: 0.10    |
| `style`   | `pin1_marker_style`       | Pin 1 marker: dot, chamfer, line. Default: dot |
| `logging` | `level`                   | trace, debug, info, warn, error. Default: warn |

## Development

### Prerequisites

- Rust 1.75+
- Cargo

### Building

```bash
cargo build
```

### Testing

```bash
cargo test
cargo clippy -- -D warnings
```

### Code Style

```bash
cargo fmt
```

## Prior Art

This project builds on the work of:

- [AltiumSharp](https://github.com/issus/AltiumSharp) - C# Altium file parser (MIT)
- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) - Python Altium library
- [python-altium](https://github.com/vadmium/python-altium) - Altium format documentation

## Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

- Follow the style guide in [STYLE.md](STYLE.md)
- Security issues: see [SECURITY.md](SECURITY.md)

## Licence

Copyright (C) 2025 The Embedded Society <https://github.com/embedded-society/altium-designer-mcp>.

GNU General Public License v3.0 — see [LICENCE](LICENCE).

## Links

- [MCP Specification](https://modelcontextprotocol.io/)
- [IPC-7351B Standard](https://www.ipc.org/)
- [Report an Issue](https://github.com/embedded-society/altium-designer-mcp/issues)
