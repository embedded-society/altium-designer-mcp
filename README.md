# altium-designer-mcp

**Your IPC-7351B calculations stay compliant. Your components stay consistent.**

An MCP server that lets AI assistants (Claude Code, Claude Desktop, VSCode Copilot) create,
read, and manage Altium Designer component libraries with IPC-7351B compliant footprints.

---

## The Problem

Creating IPC-compliant Altium components manually is time-consuming and error-prone:

| Task | Manual | With This Tool |
|------|--------|----------------|
| Create 100 components | ~2.5 weeks | ~10 minutes |
| Create 500 components | ~3 months | ~1 hour |
| Ensure IPC compliance | Error-prone | Automatic |
| Maintain style consistency | Tedious | Automatic |

**The result:** Engineers spending weeks on tedious library work instead of designing products.

## The Solution

altium-designer-mcp provides **MCP tools for AI-assisted component creation**:

```
┌─────────────────┐      ┌─────────────────┐      ┌─────────────────┐
│  AI Assistant   │      │   MCP Server    │      │  Altium Files   │
│                 │      │                 │      │                 │
│  Claude Code    │◄────►│  altium-mcp     │◄────►│  .PcbLib        │
│  Claude Desktop │      │                 │      │  .SchLib        │
│  VSCode Copilot │      │  (IPC-7351B     │      │  .DbLib         │
│                 │      │   calculations) │      │                 │
└─────────────────┘      └─────────────────┘      └─────────────────┘
```

**Key insight:** AI assistants can handle the repetitive calculations and data entry. You focus on design decisions.

### How It Works

1. **Calculate:** AI requests footprint → MCP server runs IPC-7351B calculations
2. **Generate:** Proper land patterns, courtyard, silkscreen created automatically
3. **Write:** Component saved to native Altium library files

**Every footprint follows IPC-7351B standards. Every component matches your style guide.**

---

## Current Status

**Early Development** — MCP infrastructure functional, core features in progress.

| Feature | Status |
|---------|--------|
| MCP server with stdio transport | Working |
| Tool discovery and invocation | Working |
| IPC-7351B calculations | Placeholder |
| Altium file I/O | Not yet implemented |
| Style extraction | Not yet implemented |
| Symbol generation | Not yet implemented |

---

## Who Is This For?

| Use Case | Needs This? | Why |
|----------|-------------|-----|
| **Component library creation** | **YES** | Automate tedious IPC calculations |
| **Library standardisation** | **YES** | Enforce consistent styles |
| **Batch component updates** | **YES** | Update hundreds of components |
| **Manual one-off components** | Maybe | Still useful for calculations |

---

## MCP Tools

### Currently Implemented

#### `list_package_types`

List supported IPC-7351B package families.

```json
{
  "name": "list_package_types",
  "arguments": {}
}
```

**Response:** List of supported package types (CHIP, SOIC, QFN, etc.).

#### `calculate_footprint`

Calculate IPC-7351B compliant land pattern from package dimensions.

```json
{
  "name": "calculate_footprint",
  "arguments": {
    "package_type": "CHIP",
    "body_length": 1.6,
    "body_width": 0.8,
    "terminal_length": 0.3,
    "density_level": "N"
  }
}
```

**Response:** Pad positions, courtyard, silkscreen geometry.

#### `get_ipc_name`

Generate IPC-7351B compliant component name.

```json
{
  "name": "get_ipc_name",
  "arguments": {
    "package_type": "CHIP",
    "body_length": 1.6,
    "body_width": 0.8,
    "density_level": "N"
  }
}
```

**Response:** IPC-compliant name (e.g., `CHIP_0603_N`).

### Planned Tools

| Tool | Description |
|------|-------------|
| `read_pcblib` | Read components from .PcbLib file |
| `write_pcblib` | Write components to .PcbLib file |
| `create_component` | Create complete component (footprint + symbol) |
| `extract_style` | Extract style from existing library |
| `apply_style` | Apply style to components |
| `validate_component` | Verify component against IPC rules |

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

| Section | Option | Description |
|---------|--------|-------------|
| `library_path` | — | Default path to Altium libraries |
| `ipc` | `default_density` | M (Most), N (Nominal), L (Least) |
| `ipc` | `thermal_vias` | Add vias to QFN thermal pads |
| `ipc` | `courtyard_margin` | Margin around component in mm |
| `style` | `silkscreen_line_width` | Silkscreen line width in mm |
| `style` | `assembly_line_width` | Assembly drawing width in mm |
| `style` | `pin1_marker_style` | Pin 1 marker: dot, chamfer, line |
| `logging` | `level` | trace, debug, info, warn, error |

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
- [IPC-7351B Standard](https://www.ipc.org/)
- [Report an Issue](https://github.com/embedded-society/altium-designer-mcp/issues)

## Prior Art

This project builds on the work of:

- [AltiumSharp](https://github.com/issus/AltiumSharp) — C# Altium file parser (MIT)
- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) — Python Altium library
- [python-altium](https://github.com/vadmium/python-altium) — Altium format documentation
