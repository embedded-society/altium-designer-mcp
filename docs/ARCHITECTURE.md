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
│   │   ├── mod.rs               # PcbLib module exports
│   │   ├── primitives.rs        # Pad, Track, Arc, Region, Text, Fill, etc.
│   │   ├── reader.rs            # Binary parsing
│   │   └── writer.rs            # Binary encoding
│   └── schlib/
│       ├── mod.rs               # SchLib module exports
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

See [README.md § Primitive Types](../README.md#primitive-types) for the complete primitive reference.

See [README.md § Standard Altium Layers](../README.md#standard-altium-layers) for the layer reference.

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

See [README.md § MCP Tools](../README.md#mcp-tools) for the complete tool reference with examples.

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

## Network Requirements

This is a local tool — no network access required.

| Feature | Network Required |
|---------|-----------------|
| Read/write libraries | No |
| Primitive placement | No |
| STEP model attachment | No |
