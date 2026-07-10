# altium-designer-mcp

[![CI](https://github.com/embedded-society/altium-designer-mcp/actions/workflows/ci_main.yml/badge.svg)](https://github.com/embedded-society/altium-designer-mcp/actions/workflows/ci_main.yml)
[![codecov](https://codecov.io/gh/embedded-society/altium-designer-mcp/branch/main/graph/badge.svg)](https://codecov.io/gh/embedded-society/altium-designer-mcp)

**Let an AI build your Altium libraries — it does the engineering, this tool writes the files.**

An MCP server that gives AI assistants (Claude Code, Claude Desktop, Google Antigravity, VSCode Copilot) file I/O
and primitive-placement tools for Altium Designer `.PcbLib` (footprint) and `.SchLib` (symbol)
libraries — so the AI can create and maintain *any* component, not just pre-programmed packages.

---

## The Problem

Building Altium component libraries by hand is slow and repetitive — every footprint means
looking up IPC-7351B pad sizes, courtyards, and silkscreen, then placing each primitive by
hand. AI assistants are excellent at exactly that reasoning, but they **cannot write Altium's
binary `.PcbLib`/`.SchLib` files** — an undocumented OLE compound format that is easy to
corrupt, and Altium silently refuses to open a malformed file.

| Approach | Problem |
|----------|---------|
| Draw every footprint by hand in Altium | Slow and repetitive; the AI can't touch the file |
| Ask an AI to emit the binary file directly | It produces a corrupt file Altium won't open |
| Pre-programmed footprint generators | Only the package types someone coded in advance |

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

## Who Is This For?

Anyone who **builds or maintains Altium component libraries** and uses an **MCP-capable AI
assistant**. The AI does the engineering (datasheet → dimensions → style); this server lets it
read and write the actual `.PcbLib` / `.SchLib` files.

| If you… | Then… |
|---------|-------|
| Use Claude Code, Claude Desktop, or VSCode + Copilot and design in Altium | ✅ This is for you |
| Want pre-baked generators for a fixed set of packages | ❌ Not this — the point is *any* component |
| Don't use Altium | ❌ Not applicable |

---

## Quick Start with Claude Code

> **[Claude Code Setup Guide](docs/CLAUDE_CODE_GUIDE.md)** — Complete step-by-step instructions
> for using this MCP server with Claude Code CLI on **Windows**, **Linux**, and **macOS**.

---

## Quick Start with Google Antigravity

> **[Google Antigravity Setup Guide](docs/ANTIGRAVITY_GUIDE.md)** — Step-by-step instructions
> for using this MCP server with Google Antigravity (IDE and CLI) on **Windows**, **Linux**, and **macOS**.

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

The server exposes **34 tools**, working on both `.PcbLib` (footprints) and
`.SchLib` (symbols). Every tool's full parameters and examples live in
**[docs/TOOLS.md](docs/TOOLS.md)** — this is the categorised overview.

### Read & write

| Tool | Purpose |
|------|---------|
| [`read_pcblib`](docs/TOOLS.md#read_pcblib) | Read footprints from a `.PcbLib`. |
| [`write_pcblib`](docs/TOOLS.md#write_pcblib) | Write footprints to a `.PcbLib`. |
| [`read_schlib`](docs/TOOLS.md#read_schlib) | Read symbols from a `.SchLib`. |
| [`write_schlib`](docs/TOOLS.md#write_schlib) | Write symbols to a `.SchLib`. |

### Inspect & visualise

| Tool | Purpose |
|------|---------|
| [`list_components`](docs/TOOLS.md#list_components) | List component names (paginated). |
| [`get_component`](docs/TOOLS.md#get_component) | Get one component's full data. |
| [`search_components`](docs/TOOLS.md#search_components) | Search across libraries by regex/glob. |
| [`component_exists`](docs/TOOLS.md#component_exists) | Check whether components exist. |
| [`render_footprint`](docs/TOOLS.md#render_footprint) | ASCII-art preview of a footprint. |
| [`render_symbol`](docs/TOOLS.md#render_symbol) | ASCII-art preview of a symbol. |
| [`extract_style`](docs/TOOLS.md#extract_style) | Extract styling from an existing library. |

### Compare

| Tool | Purpose |
|------|---------|
| [`diff_libraries`](docs/TOOLS.md#diff_libraries) | Compare two library files. |
| [`compare_components`](docs/TOOLS.md#compare_components) | Diff two specific components. |

### Edit in place

| Tool | Purpose |
|------|---------|
| [`update_component`](docs/TOOLS.md#update_component) | Update a component, preserving its position. |
| [`update_pad`](docs/TOOLS.md#update_pad) | Update one pad's properties. |
| [`update_primitive`](docs/TOOLS.md#update_primitive) | Update one primitive (track/arc/text/fill/region). |
| [`batch_update`](docs/TOOLS.md#batch_update) | Batch updates across all components. |
| [`reorder_components`](docs/TOOLS.md#reorder_components) | Reorder components in a library. |
| [`manage_schlib_parameters`](docs/TOOLS.md#manage_schlib_parameters) | List/get/set/remove SchLib parameters. |
| [`manage_schlib_footprints`](docs/TOOLS.md#manage_schlib_footprints) | Manage footprint links in SchLib symbols. |

### Manage components

| Tool | Purpose |
|------|---------|
| [`delete_component`](docs/TOOLS.md#delete_component) | Delete one or more components. |
| [`copy_component`](docs/TOOLS.md#copy_component) | Duplicate a component within a library. |
| [`rename_component`](docs/TOOLS.md#rename_component) | Rename a component (atomic). |
| [`copy_component_cross_library`](docs/TOOLS.md#copy_component_cross_library) | Copy a component to another library. |
| [`bulk_rename`](docs/TOOLS.md#bulk_rename) | Pattern-based multi-rename. |

### Library operations

| Tool | Purpose |
|------|---------|
| [`merge_libraries`](docs/TOOLS.md#merge_libraries) | Merge multiple libraries into one. |
| [`write_libpkg`](docs/TOOLS.md#write_libpkg) | Write a `.LibPkg` project grouping libraries for IntLib compilation. |
| [`export_library`](docs/TOOLS.md#export_library) | Export to JSON/CSV. |
| [`import_library`](docs/TOOLS.md#import_library) | Import from JSON (inverse of export). |
| [`validate_library`](docs/TOOLS.md#validate_library) | Validate for common issues. |
| [`repair_library`](docs/TOOLS.md#repair_library) | Remove orphaned data. |
| [`extract_step_model`](docs/TOOLS.md#extract_step_model) | Extract embedded STEP 3D models. |

### Backups & safety

| Tool | Purpose |
|------|---------|
| [`list_backups`](docs/TOOLS.md#list_backups) | List automatic backups. |
| [`restore_backup`](docs/TOOLS.md#restore_backup) | Restore from a backup. |

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
| **Pie** | Filled circular sector (arc geometry plus fill) |
| **Image** | Embedded or linked raster picture with a bounding box |
| **Ellipse** | Ellipse or circle (filled or unfilled) |
| **EllipticalArc** | Elliptical arc segment with fractional radii |
| **Bezier** | Cubic Bezier curve (4 control points) |
| **Label** | Text label (RECORD=4) |
| **Text** | Text annotation (RECORD=3) |
| **TextFrame** | Bordered multi-line text box (word-wrap, alignment) |
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

## Prior Art & Acknowledgements

This project stands on the shoulders of several excellent open-source efforts, and we're grateful
for each:

- **[AltiumSharp](https://github.com/issus/AltiumSharp)** (MIT) — the most complete open Altium
  reader/writer. Used as the authoritative reference (its DTOs, binary serializers, and golden
  `TestData`) for verifying our binary format against ground truth.
- **[pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib)** — an independent Python reader, used
  as our CI **readability oracle** (`tests/integration/`) to check that generated files actually
  parse.
- **[python-altium](https://github.com/vadmium/python-altium)** — early Altium format
  documentation.
- **[coffeenmusic/altium-mcp](https://github.com/coffeenmusic/altium-mcp)** (MIT) — an MCP server
  that drives the **live** Altium application. It's the complement to this project (we generate and
  edit library files *offline*; it controls a running session). We adapted its RunScript launch +
  file-based bridge pattern for our on-site Altium automation
  ([`scripts/altium/`](scripts/altium/)).
