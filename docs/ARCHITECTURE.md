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
├── util.rs                      # Path redaction, CSV escaping, UniqueId generation
│
├── config/                      # Configuration
│   ├── mod.rs                   # Module exports
│   └── settings.rs              # Config file parsing + defaults
│
├── security/                    # Safety controls
│   ├── mod.rs                   # Module exports
│   ├── audit.rs                 # Append-only audit log for mutating tools
│   └── rate_limit.rs            # Token-bucket rate limiter (mutating tools)
│
├── altium/                      # Altium file I/O
│   ├── mod.rs                   # Shared helpers: Windows-1252, OLE names, atomic save
│   ├── error.rs                 # Altium-specific errors (path-sanitised Display)
│   ├── bytes.rs                 # Bounds-checked little-endian scalar readers
│   ├── base64_opt.rs            # Serde base64 codec for embedded image bytes
│   ├── framing.rs               # Shared block / Pascal-string / C-string frames
│   ├── text.rs                  # TextJustification (shared enum)
│   ├── serde_round.rs           # 6-decimal f64 rounding on serialise
│   ├── libpkg.rs                # .LibPkg project-file generator
│   ├── pcblib/
│   │   ├── mod.rs               # PcbLib + Footprint types, CRUD
│   │   ├── read_io.rs           # OLE stream orchestration (read)
│   │   ├── write_io.rs          # OLE stream orchestration (write)
│   │   ├── reader/              # Binary parsing (dispatch, per-primitive, 3D models)
│   │   ├── writer.rs            # Binary encoding (byte templates)
│   │   ├── primitives/          # Pad, Via, Track, Arc, Region, Text, Fill, bodies
│   │   ├── flags.rs             # On-disk flag-word bits
│   │   ├── units.rs             # mm ↔ Altium internal units
│   │   └── assets/              # Captured Library/Data stack + FileVersionInfo
│   └── schlib/                  # (mirrors pcblib/ — same module shape)
│       ├── mod.rs               # SchLib + Symbol types, CRUD
│       ├── read_io.rs           # OLE stream orchestration (read)
│       ├── write_io.rs          # OLE stream orchestration (write)
│       ├── reader/              # Record parsing (dispatch + per-record parsers)
│       ├── writer.rs            # Record encoding (omit-when-default)
│       ├── primitives/          # Pin, shapes, text, footprint models
│       ├── coord.rs             # Fractional (_Frac) coordinate codec
│       ├── pin_aux.rs           # PinFrac / PinSymbolLineWidth aux streams
│       └── storage.rs           # /Storage stream + compressed-storage framing
│
└── mcp/                         # MCP server implementation
    ├── mod.rs                   # Module exports
    ├── server.rs                # JSON-RPC dispatch, path validation, backups, audit
    ├── protocol.rs              # MCP protocol types
    ├── transport.rs             # stdio transport
    ├── tool_definitions.rs      # Tool schemas (source of truth for docs/TOOLS.md)
    ├── tool_docs.rs             # docs/TOOLS.md generator + drift guard (test-only)
    └── tools/                   # One file per tool family (read_write, compare, …)
```

---

## Data Flow: Component Creation

See [README § How It Works](../README.md#how-it-works) for the sequence diagram of a
component-creation call (engineer → AI → MCP server): the AI computes the geometry and
calls `write_pcblib` / `write_schlib`; the server writes the OLE compound document.

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

See [docs/SECURITY.md](SECURITY.md) for the threat model, the concrete controls
(path confinement, error sanitisation, rate limiting, bounded decompression), and
where each lives in the source.

---

## Network Requirements

This is a local tool — no network access required.

| Feature | Network Required |
|---------|-----------------|
| Read/write libraries | No |
| Primitive placement | No |
| STEP model attachment | No |
