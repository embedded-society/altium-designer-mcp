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

See `docs/ARCHITECTURE.md` § Component Overview for the maintained source tree —
it is the single source of truth; do not duplicate it here.

Key orientation points:

- `src/altium/` — file-format layer: shared framing/encoding helpers at the root,
  `pcblib/` (binary records over byte templates) and `schlib/` (text records,
  omit-when-default) beneath.
- `src/mcp/` — server layer: dispatch + security choke-points in `server.rs`, one
  handler file per tool family in `tools/`, tool schemas in `tool_definitions.rs`
  (the source of truth for the generated `docs/TOOLS.md`).
- `src/security/` — rate limiting + audit log for mutating tools.

## MCP Tools

See [README.md § MCP Tools](../README.md#mcp-tools) for the complete tool reference.

## Primitives

See [README.md § Primitive Types](../README.md#primitive-types) for the complete primitive reference.

## Off Limits

**`CODE_OF_CONDUCT.md`** — Do not modify.
