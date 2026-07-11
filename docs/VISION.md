# Vision: AI-Assisted PCB Component Library Management

This document describes the architectural vision for altium-designer-mcp.

## Core Principle

**The AI handles the intelligence. The tool handles file I/O.**

```text
┌─────────────────────────────────────────────────────────────────────────┐
│  RESPONSIBILITY SPLIT                                                    │
│                                                                         │
│  AI (Claude, etc.)                    MCP Server (this tool)            │
│  ─────────────────                    ──────────────────────            │
│  • IPC-7351B calculations             • Read .PcbLib/.SchLib files      │
│  • Package layout decisions           • Write .PcbLib/.SchLib files     │
│  • Style choices                      • Delete components from files    │
│  • Datasheet interpretation           • Primitive placement             │
│  • Design rule knowledge              • STEP model attachment           │
│                                       • OLE document handling           │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## How It Works

See [README § How It Works](../README.md#how-it-works) for the sequence diagram of an
AI-assisted component-creation call (engineer → AI → MCP server).

---

## MCP Tools

See [README.md § MCP Tools](../README.md#mcp-tools) for the complete tool reference with examples.

---

## Primitives

The AI provides complete primitive definitions. The tool just writes them.

See [README.md § Primitive Types](../README.md#primitive-types) for the complete primitive reference.

---

## STEP Models

STEP models are **attached**, not generated.

The tool links existing STEP files to footprints. For parametric 3D model
generation, use a dedicated mechanical MCP server (future project).

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

## Scope & Non-Goals

**This tool does:** read, write, and edit Altium `.PcbLib` / `.SchLib` libraries — place
primitives, attach STEP models, and validate the result — i.e. the file I/O an AI cannot do
for itself.

**This tool deliberately does NOT:**

- **Do the engineering.** IPC-7351B sizing, courtyards, and styling are the AI's job — see the
  [Core Principle](#core-principle).
- **Replace Altium Designer.** It generates *source* libraries; compiling to an IntLib,
  rendering, and manufacturing happen inside Altium.
- **Capture schematics or boards.** It manages component *libraries*, not `.SchDoc` / `.PcbDoc`
  designs.
- **Touch the network.** All I/O is local files (see [ARCHITECTURE § Network Requirements](ARCHITECTURE.md#network-requirements)).

---

## Design Principles

1. **The AI owns the intelligence; the tool owns the bytes.** The whole point of the
   [Core Principle](#core-principle) — keep the tool package-agnostic.
2. **Fail loudly, never corrupt silently.** Altium refuses to open a malformed file, so the
   writer is checked against an independent Altium-readability oracle, not just its own reader.
3. **Round-trip fidelity.** Reading a library and writing it back preserves everything the
   model represents — coordinates, parameters, and unique IDs.
4. **Byte-exact Altium output.** Windows-1252 strings and the precise binary record layout —
   match Altium on disk, not an approximation.
5. **Safe by default.** Validate and canonicalise every path, back up before mutating, and keep
   file paths out of error messages.

---

## IPC Standards

The AI applies industry standards when calculating footprints:

| Standard | Purpose |
|----------|---------|
| [IPC-7351B](https://shop.ipc.org/) | Surface mount land pattern design |
| [IPC-2221](https://shop.ipc.org/) | Printed board design (through-hole) |

See [AI_WORKFLOW.md](AI_WORKFLOW.md#ipc-standards-reference) for detailed IPC reference.
