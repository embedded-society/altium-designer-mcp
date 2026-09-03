# altium-designer-mcp

[![CI](https://github.com/embedded-society/altium-designer-mcp/actions/workflows/ci_main.yml/badge.svg)](https://github.com/embedded-society/altium-designer-mcp/actions/workflows/ci_main.yml)
[![codecov](https://codecov.io/gh/embedded-society/altium-designer-mcp/branch/main/graph/badge.svg)](https://app.codecov.io/gh/embedded-society/altium-designer-mcp)

**Let an AI build your Altium libraries — it does the engineering, this tool writes the files.**

An MCP server that gives AI assistants (Claude Code, Claude Desktop, Cursor, Antigravity, VS Code Copilot — any MCP client) file I/O
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
| Use Claude Code, Claude Desktop, Cursor, Antigravity, VS Code + Copilot — [any MCP client](docs/CLIENT_SETUP.md) — and design in Altium | ✅ This is for you |
| Want pre-baked generators for a fixed set of packages | ❌ Not this — the point is *any* component |
| Don't use Altium | ❌ Not applicable |

---

## Quick Start

> **[Client Setup](docs/CLIENT_SETUP.md)** — verified configuration for Claude Code, Claude
> Desktop, Google Antigravity, Cursor, VS Code, GitHub Copilot CLI, Windsurf, Cline, Roo Code,
> Kiro, JetBrains, Zed, Gemini CLI, Codex CLI, Continue, Goose, OpenCode and any other stdio
> MCP client, plus troubleshooting — on **Windows**, **Linux**, and **macOS**.
>
> **[Using the server](docs/USAGE.md)** — what to ask for once it is connected: example
> workflows, prompts and tips, identical for every client.

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
│    │                         │  { status: "success" }       │               │
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
| Octagonal | `"octagonal"` | Eight-sided pads (chamfered corners) |

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
| **Label** | Text string (RECORD=4) — the only free text on a symbol |
| **IeeeSymbol** | IEEE symbol glyph (RECORD=3): a dot, a clock, an active-low input, … |
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
| Mechanical 1–32 | User-defined mechanical layers |
| Drill Guide | Drill hole markers |
| Drill Drawing | Drill chart/table |
| Keep-Out Layer | Routing exclusion zones |

A layer may be named as Altium spells it (`Top Overlay`, `Mechanical 13`) or in camel
case (`TopOverlay`, `Mechanical13`), in any case; every tool accepts the same spellings.

---

## Installation

**Prebuilt binaries** for Linux (x86_64), macOS (aarch64) and Windows (x86_64) are on the
[Releases page](https://github.com/embedded-society/altium-designer-mcp/releases) — each
archive bundles a setup README plus [`docs/CLIENT_SETUP.md`](docs/CLIENT_SETUP.md), which
wires the server into every MCP client we know of.

**Claude Desktop users** need no archive at all: install the one-click extension
`altium-designer-mcp.mcpb` from the same page (older builds: the identical
`altium-designer-mcp.dxt`) via Settings → Extensions → Advanced settings →
Install Extension… — see [CLIENT_SETUP.md § Claude Desktop](docs/CLIENT_SETUP.md#claude-desktop).

**In a container** — for a Linux box, a NAS or a CI job that generates libraries into a
mounted folder (Altium itself never needs to be inside): the repository's `Dockerfile`
produces the same `--locked` release build as the published binaries.

```bash
docker build -t altium-designer-mcp .
docker run -i --rm -v /path/to/libraries:/libraries altium-designer-mcp
```

The mounted `/libraries` folder is the container's whole allow-list. In a client's
configuration that is `"command": "docker"` with
`"args": ["run", "-i", "--rm", "-v", "/path/to/libraries:/libraries", "altium-designer-mcp"]`.

To build from source instead, see
[CONTRIBUTING.md § Development Setup](CONTRIBUTING.md#development-setup); an optimised
binary comes from `cargo build --release` and lands at `target/release/altium-designer-mcp`.

### Verifying a downloaded release

Released archives are built by GitHub Actions and carry a signed
[SLSA build provenance](https://slsa.dev/) attestation, so a download can be traced
back to the workflow run and commit that produced it:

```bash
gh attestation verify <archive> --repo embedded-society/altium-designer-mcp
sha256sum --check --ignore-missing SHA256SUMS.txt
```

The binaries are not code-signed, so Windows SmartScreen and macOS Gatekeeper warn on
first run (on macOS, right-click → Open). The attestation is the stronger check.
See [docs/RELEASING.md](docs/RELEASING.md) for how releases are produced.

### Command-Line Usage

```bash
altium-designer-mcp [OPTIONS] [CONFIG_FILE]
```

| Option | Description |
|--------|-------------|
| `CONFIG_FILE` | Path to configuration file (optional, uses default location if omitted) |
| `--allow <DIR>...` | Grant access to library folders directly (repeatable). Adds to the config file's `allowed_paths`, and works with no config file at all — the other settings then take their defaults |
| `-v`, `--verbose` | Increase logging verbosity (`-v` info, `-vv` debug, `-vvv` trace) |
| `-q`, `--quiet` | Decrease logging verbosity (only show errors) |
| `-h`, `--help` | Print help information |
| `-V`, `--version` | Print version information |

### Connecting an AI client

Every MCP client needs the same two absolute paths — the binary and your config file — and
differs only in where they are written. The standard block most clients read:

```json
{
    "mcpServers": {
        "altium": {
            "command": "/usr/local/bin/altium-designer-mcp",
            "args": ["/home/you/.altium-designer-mcp/config.json"]
        }
    }
}
```

Where that goes for Claude Desktop, Cursor, VS Code, Windsurf, Cline, Zed, JetBrains,
Gemini CLI, Codex CLI and the rest — and what to do when a client cannot see the server —
is in [docs/CLIENT_SETUP.md](docs/CLIENT_SETUP.md). Use absolute paths: clients do not
search `PATH` or expand `~` for you.

---

## Configuration

The server reads one JSON file — or none: `altium-designer-mcp --allow <DIR>` grants
folders on the command line and runs on defaults for everything else, which is how the
Claude Desktop extension starts it. Configuration file location:

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
| `allowed_paths` | Array of directory paths where library files can be accessed; `--allow` adds to it (default when neither grants anything: the current working directory) |
| `logging.level` | Log level: trace, debug, info, warn, error (default: warn) |
| `logging.audit_log_path` | Path to an append-only JSON-lines audit log of destructive operations (default: null — no audit log is written) |
| `rate_limit.max_burst` | Maximum burst of mutating operations before throttling; read-only tools are never rate limited (default: 120) |
| `rate_limit.refill_per_sec` | Token-bucket refill rate for mutating operations, in tokens per second (default: 30.0) |

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

When copying or merging components between libraries:

- **Embedded models** travel with the component — `copy_component_cross_library` and
  `merge_libraries` both copy the referenced model streams into the target (a model shared by
  several footprints is copied once), so the bodies still resolve after the move.
- **External model references**: `copy_component_cross_library` removes them with a warning by
  default, since a path relative to the source library rarely resolves elsewhere — pass
  `preserve_external_paths=true` to keep them. `merge_libraries` carries them unchanged.

Embedding a model in the source library is the reliable way to keep 3D data through any copy.

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
- `repair_library`
- `manage_schlib_parameters`
- `manage_schlib_footprints`
- `write_pcblib` / `write_schlib` (when overwriting)
- `import_library` (when overwriting)
- `restore_backup` (the current file, before the chosen backup replaces it)

**Managing backups:** Use `list_backups` to view available backups and `restore_backup` to
recover from a previous version.

**Dry-run support:** These operations support `dry_run=true` to preview changes
without modifying files:

- `delete_component` — preview which components would be deleted
- `update_component` — preview component replacement changes
- `update_pad` / `update_primitive` — preview property changes
- `bulk_rename` — preview name changes
- `repair_library` — preview orphaned references to remove
- `batch_update` — preview library-wide updates
- `copy_component` / `rename_component` / `merge_libraries`

---

## Notes

### Long Component Names

Component names longer than 31 characters are supported. The OLE Compound File format limits
storage names to 31 characters, so longer names are automatically truncated internally while
the full name is preserved in component parameters. This is handled transparently — you can
use any length component name and it will be preserved on read/write roundtrips.

---

## Privacy Policy

`altium-designer-mcp` is a local tool and collects nothing.

- **Data collection**: none. The server has no network access, no telemetry and no
  analytics; it never contacts any service, including this project's.
- **Usage and storage**: it reads and writes only the library files inside the folders
  you grant (`allowed_paths` or `--allow`), plus the timestamped `.bak` copies it makes
  beside them before a change. The optional audit log (`logging.audit_log_path`) is a
  local file you choose, holding tool names, file names and outcomes — never library
  contents.
- **Third-party sharing**: none. Nothing leaves your machine.
- **Data retention**: the files and backups stay until you delete them; backups are
  capped at the five most recent per library.
- **Contact**: <matejg03@gmail.com>, or a [GitHub issue](https://github.com/embedded-society/altium-designer-mcp/issues)
  for anything that need not be private.

---

## Documentation

| For… | Read |
|------|------|
| Wiring the server into your AI client | [docs/CLIENT_SETUP.md](docs/CLIENT_SETUP.md) — one section per client |
| What to ask for once it is connected | [docs/USAGE.md](docs/USAGE.md) — workflows, prompts, tips |
| Telling the AI how to use it well | [docs/AGENT_GUIDE.md](docs/AGENT_GUIDE.md) (paste into a project brief), [docs/AI_WORKFLOW.md](docs/AI_WORKFLOW.md) |
| Every tool, parameter and example | [docs/TOOLS.md](docs/TOOLS.md); error messages in [docs/errors.md](docs/errors.md) |
| Why it is built this way | [docs/VISION.md](docs/VISION.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| The file formats, byte by byte | [docs/PCBLIB_FORMAT.md](docs/PCBLIB_FORMAT.md), [docs/SCHLIB_FORMAT.md](docs/SCHLIB_FORMAT.md) |
| Security model and threat analysis | [docs/SECURITY.md](docs/SECURITY.md) (reporting: [SECURITY.md](SECURITY.md)) |
| How releases are built and verified | [docs/RELEASING.md](docs/RELEASING.md) |

---

## Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

- Follow the style guide in [STYLE.md](STYLE.md)
- Security issues: see [SECURITY.md](SECURITY.md)

---

## Development

```bash
cargo test
```

Write-path tests generate their own data programmatically; reader tests parse the committed
Altium-authored golden fixtures (see [Sample Files](#sample-files)). Temporary files are
created in `.tmp/` (git-ignored) and automatically cleaned up.

The full build, formatting, and lint commands are canonical in
[CONTRIBUTING.md § Development Setup](CONTRIBUTING.md#development-setup).

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

Altium-authored sample libraries are committed under `scripts/samples/` as **golden fixtures**:
the reader tests (`tests/samples_pcblib.rs`, `tests/samples_schlib.rs`) parse them in CI as
ground truth. The PowerShell/DelphiScript tooling that (re)generates them needs a real Altium
installation and is manual-only.

See [scripts/README.md](scripts/README.md) for details on the sample files and the on-site tooling.

---

## Prior Art & Acknowledgements

This project stands on the shoulders of several excellent open-source efforts, and we're grateful
for each:

- **[AltiumSharp](https://github.com/issus/AltiumSharp)** (MIT) — the most complete open Altium
  reader/writer. Used as the authoritative reference (its DTOs, binary serialisation code, and golden
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
