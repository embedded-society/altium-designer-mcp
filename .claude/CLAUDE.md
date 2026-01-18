# altium-designer-mcp — AI Assistant Context

## Core Principle

**The AI handles the intelligence. The tool handles file I/O.**

See `docs/VISION.md` for the full responsibility split.

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

See [README.md § MCP Tools](../README.md#mcp-tools) for the complete tool reference.

## Primitives

See [README.md § Primitive Types](../README.md#primitive-types) for the complete primitive reference.

## Off Limits

**`CODE_OF_CONDUCT.md`** — Do not modify.
