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
│  • Style choices                      • Primitive placement             │
│  • Datasheet interpretation           • STEP model attachment           │
│  • Design rule knowledge              • OLE document handling           │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## How It Works

```text
┌─────────────────────────────────────────────────────────────────────────┐
│  AI-ASSISTED COMPONENT CREATION                                         │
│                                                                         │
│  Engineer                    AI                         MCP Server      │
│    │                         │                              │           │
│    │  "Create 0603 resistor" │                              │           │
│    ├────────────────────────►│                              │           │
│    │                         │                              │           │
│    │                         │  AI reasons about:           │           │
│    │                         │  • IPC-7351B pad sizes       │           │
│    │                         │  • Courtyard margins         │           │
│    │                         │  • Silkscreen style          │           │
│    │                         │                              │           │
│    │                         │  write_pcblib(primitives)    │           │
│    │                         ├─────────────────────────────►│           │
│    │                         │                              │           │
│    │                         │                              │  Writes   │
│    │                         │                              │  OLE file │
│    │                         │                              │           │
│    │                         │◄─────────────────────────────┤           │
│    │                         │  { success: true }           │           │
│    │                         │                              │           │
│    │  "Done! Footprint       │                              │           │
│    │   created in PcbLib"    │                              │           │
│    │◄────────────────────────┤                              │           │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## MCP Tools

| Tool | Description |
|------|-------------|
| `read_pcblib` | Read footprints and primitives from .PcbLib |
| `read_schlib` | Read symbols and primitives from .SchLib |
| `write_pcblib` | Write footprints (defined by primitives) to .PcbLib |
| `write_schlib` | Write symbols (defined by primitives) to .SchLib |
| `list_components` | List component names in a library |
| `extract_style` | Extract styling information from existing libraries |

---

## Primitives

The AI provides complete primitive definitions. The tool just writes them.

### Footprint Primitives

- **Pad**: Position, size, shape, designator, layer
- **Track**: Start/end points, width, layer
- **Arc**: Center, radius, angles, width, layer
- **Region**: Vertices, layer
- **Text**: Position, content, size, layer

### Symbol Primitives

- **Pin**: Position, designator, name, orientation, electrical type
- **Rectangle**: Position, size, fill
- **Line**: Start/end points
- **Polyline**: Multiple connected line segments
- **Arc**: Centre, radius, angles
- **Ellipse**: Centre, radii, fill
- **Label**: Position, content, font

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

## IPC Standards

The AI applies industry standards when calculating footprints:

| Standard | Purpose |
|----------|---------|
| [IPC-7351B](https://shop.ipc.org/) | Surface mount land pattern design |
| [IPC-2221](https://shop.ipc.org/) | Printed board design (through-hole) |

See [AI_WORKFLOW.md](AI_WORKFLOW.md#ipc-standards-reference) for detailed IPC reference.
