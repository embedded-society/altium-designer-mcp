# altium-designer-mcp — AI Assistant Context

## Core Principle

**The AI handles the intelligence. The tool handles file I/O.**

| Responsibility | Owner |
|---------------|-------|
| IPC-7351B calculations | AI |
| Package layout decisions | AI |
| Style choices | AI |
| Reading/writing Altium files | This tool |
| Primitive placement | This tool |

## What Is This Project?

An MCP server that provides file I/O and primitive placement tools for Altium Designer
component libraries. The AI calculates footprint dimensions; the tool writes them to files.

See `docs/VISION.md` for the full architecture.

## Quick Reference

| Resource | Location |
|----------|----------|
| Vision | `docs/VISION.md` |
| Architecture | `docs/ARCHITECTURE.md` |
| AI Workflow | `docs/AI_WORKFLOW.md` |

## Critical Rules

### NEVER Do These

1. **NEVER write arbitrary files outside library paths**
2. **NEVER expose internal file paths in error messages**
3. **NEVER push to main**

### ALWAYS Do These

1. **Validate file paths** before reading/writing
2. **Use sensible defaults** when config is missing
3. **Sanitise paths** in error messages

## Project Structure

```
src/
├── lib.rs              # Library crate root
├── main.rs             # CLI entry point
├── error.rs            # Top-level error types
├── config/             # Configuration
│   ├── mod.rs
│   └── settings.rs
├── mcp/                # MCP server
│   ├── mod.rs
│   ├── server.rs       # Tool definitions & handlers
│   ├── protocol.rs     # JSON-RPC types
│   └── transport.rs    # Stdio transport
└── altium/             # Altium file I/O
    ├── mod.rs
    ├── error.rs        # Altium-specific errors
    ├── pcblib/         # .PcbLib read/write
    │   ├── mod.rs
    │   ├── primitives.rs
    │   ├── reader.rs
    │   └── writer.rs
    └── schlib/         # .SchLib read/write
        ├── mod.rs
        ├── primitives.rs
        ├── reader.rs
        └── writer.rs
```

## MCP Tools

| Tool | Description |
|------|-------------|
| `read_pcblib` | Read footprints and primitives from .PcbLib |
| `read_schlib` | Read symbols and primitives from .SchLib |
| `list_components` | List component names in a library |
| `extract_style` | Extract styling info from existing libraries |
| `write_pcblib` | Write footprints to .PcbLib |
| `write_schlib` | Write symbols to .SchLib |

## Primitives

The AI provides primitive definitions. The tool writes them.

**Footprint**: Pads, tracks, arcs, regions, text
**Symbol**: Pins, rectangles, lines, arcs, text

## Off Limits

**`CODE_OF_CONDUCT.md`** — Do not modify.
