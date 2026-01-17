# Architecture: altium-designer-mcp

This document describes the architecture of the MCP server.

## Core Principle

**The AI handles the intelligence. The tool handles file I/O.**

See [VISION.md](VISION.md) for the full responsibility split and architectural rationale.

This architecture means the AI can create **any footprint** — not just pre-programmed
package types. The tool is package-agnostic.

---

## Component Overview

```text
src/
├── lib.rs                       # Library crate root
├── main.rs                      # CLI entry point
├── error.rs                     # Top-level error types
│
├── config/                      # Configuration
│   ├── mod.rs                   # Module exports
│   └── settings.rs              # Config file parsing + defaults
│
├── altium/                      # Altium file I/O
│   ├── mod.rs                   # Module exports
│   ├── error.rs                 # Altium-specific errors
│   ├── pcblib/
│   │   ├── mod.rs               # PcbLib read/write
│   │   └── primitives.rs        # Pad, Track, Arc, Region, Text, Layer
│   └── schlib/
│       ├── mod.rs               # SchLib read/write
│       ├── primitives.rs        # Pin, Rectangle, Line, Arc, Ellipse, etc.
│       ├── reader.rs            # Binary parsing
│       └── writer.rs            # Binary encoding
│
└── mcp/                         # MCP server implementation
    ├── mod.rs                   # Module exports
    ├── server.rs                # JSON-RPC server + tool handlers
    ├── protocol.rs              # MCP protocol types
    └── transport.rs             # stdio transport
```

---

## Data Flow: Component Creation

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ CREATE FOOTPRINT: AI calculates dimensions, tool writes file                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Engineer              AI                            MCP Server             │
│    │                    │                                │                  │
│    │  "Create 0603"     │                                │                  │
│    ├───────────────────►│                                │                  │
│    │                    │                                │                  │
│    │                    │  AI reasons about:             │                  │
│    │                    │  • IPC-7351B formulas          │                  │
│    │                    │  • Pad size calculation        │                  │
│    │                    │  • Courtyard margins           │                  │
│    │                    │  • Silkscreen placement        │                  │
│    │                    │                                │                  │
│    │                    │  write_pcblib                  │                  │
│    │                    │  { filepath, footprints: [{    │                  │
│    │                    │      name, pads, tracks,       │                  │
│    │                    │      arcs, regions, text }]}   │                  │
│    │                    ├───────────────────────────────►│                  │
│    │                    │                                │                  │
│    │                    │                                │  Write OLE file  │
│    │                    │                                │  with primitives │
│    │                    │                                │                  │
│    │                    │◄───────────────────────────────┤                  │
│    │                    │  { success: true }             │                  │
│    │                    │                                │                  │
│    │◄───────────────────┤                                │                  │
│    │  "Footprint        │                                │                  │
│    │   RESC1608X55N     │                                │                  │
│    │   created!"        │                                │                  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Primitive Types

The AI provides complete primitive definitions. The tool writes them.

### Footprint Primitives

| Primitive | Properties |
|-----------|------------|
| **Pad** | designator, x, y, width, height, shape, layer, hole_size, rotation |
| **Track** | x1, y1, x2, y2, width, layer |
| **Arc** | x, y, radius, start_angle, end_angle, width, layer |
| **Region** | vertices[], layer |
| **Text** | x, y, text, height, layer, rotation |
| **Model3D** | filepath, x_offset, y_offset, z_offset, rotation |

### Standard Altium Layers

| Layer | Typical Usage |
|-------|---------------|
| Top Layer | Copper pads |
| Multi-Layer | Through-hole pads (all layers) |
| Top Overlay | Silkscreen |
| Top Paste | Solder paste |
| Top Solder | Solder mask |
| Top Assembly | Assembly outline |
| Top 3D Body | 3D body outline |
| Top Courtyard | Courtyard (IPC-7351) |

---

## Altium File Format

Altium libraries use OLE Compound File Binary (CFB) format:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ .PcbLib File Structure                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  OLE Compound File                                                          │
│  ├── /FileHeader                  # Library metadata (ASCII)                │
│  ├── /Footprint1/                 # Storage for first footprint             │
│  │   ├── Data                     # Binary primitive data                   │
│  │   └── Parameters               # Component parameters (ASCII)            │
│  ├── /Footprint2/                                                           │
│  │   ├── Data                                                               │
│  │   └── Parameters                                                         │
│  └── ...                                                                    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

The `Data` stream contains binary records for each primitive. The exact binary
format is being reverse-engineered from existing libraries and prior art
(AltiumSharp, python-altium).

---

## MCP Tools

| Tool | Description |
|------|-------------|
| `read_pcblib` | Read footprints and primitives from .PcbLib |
| `write_pcblib` | Write footprints (defined by primitives) to .PcbLib |
| `read_schlib` | Read symbols and primitives from .SchLib |
| `write_schlib` | Write symbols to .SchLib |
| `list_components` | List component names in a library |
| `extract_style` | Extract styling information from existing libraries |

---

## Security Considerations

### File Access

- The MCP server only accesses paths configured in the config file
- No arbitrary file system access
- Path traversal attacks are prevented

### Input Validation

- Primitive coordinates and dimensions are validated
- Invalid inputs return clear error messages
- No code execution from user input

### Error Handling

- Internal errors are logged but not exposed to users
- Sensitive paths are sanitised in error messages
- Stack traces are only shown in debug mode

---

## Why This Architecture?

Previous approach encoded IPC-7351B into calculators. This was over-engineered:

| Old Approach | New Approach |
|-------------|--------------|
| Tool calculates pad sizes | AI calculates pad sizes |
| Tool has package-specific code | Tool is package-agnostic |
| Need to add code for each package | AI handles any package |
| Complex codebase | Simple file I/O |

The AI already knows IPC-7351B. We don't need to duplicate that knowledge.

---

## Network Requirements

This is a local tool — no network access required.

| Feature | Network Required |
|---------|-----------------|
| Read/write libraries | No |
| Primitive placement | No |
| STEP model attachment | No |
