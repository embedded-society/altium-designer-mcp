# Vision: AI-Assisted PCB Component Library Management

This document describes the architectural vision for altium-designer-mcp.

## The Problem

Creating IPC-compliant Altium components manually is time-consuming and error-prone:

```
┌─────────────────────────────────────────────────────────────────────────┐
│  MANUAL COMPONENT CREATION (Current State)                              │
│                                                                         │
│  Engineer                                                               │
│    │                                                                    │
│    ├── Hunt through datasheet for dimensions      (~15 min)             │
│    ├── Look up IPC-7351B tables                   (~10 min)             │
│    ├── Calculate land pattern manually            (~10 min)             │
│    ├── Create footprint in Altium                 (~20 min)             │
│    ├── Create schematic symbol                    (~15 min)             │
│    ├── Fill in 40+ parameter fields               (~15 min)             │
│    └── Review and verify                          (~10 min)             │
│                                                                         │
│  Total: ~1.5 hours per component                                        │
│  100 components = ~2.5 weeks of tedious work                            │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**Key insight:** This is exactly the kind of repetitive, rule-based task that AI excels at.

---

## The Solution: AI-Assisted Automation

```
┌─────────────────────────────────────────────────────────────────────────┐
│  AI-ASSISTED COMPONENT CREATION                                         │
│                                                                         │
│  Engineer                    MCP Server                    Altium       │
│    │                            │                            │          │
│    │  "Create a 0603 resistor"  │                            │          │
│    ├───────────────────────────►│                            │          │
│    │                            │                            │          │
│    │                            │  IPC-7351B calculations    │          │
│    │                            │  ─────────────────────►    │          │
│    │                            │                            │          │
│    │                            │  Generate footprint        │          │
│    │                            │  Generate symbol           │          │
│    │                            │  Fill parameters           │          │
│    │                            │  ─────────────────────►    │          │
│    │                            │                            │          │
│    │  "Done! Component ready"   │                            │          │
│    │◄───────────────────────────┤                            │          │
│    │                            │                            │          │
│  Total: ~2 minutes per component                                        │
│  100 components = ~3 hours (vs 2.5 weeks)                               │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## How It Works

### MCP Integration

The MCP (Model Context Protocol) server provides tools that AI assistants can call:

```
┌─────────────────────────────────────────────────────────────────────────┐
│  MCP TOOL ARCHITECTURE                                                  │
│                                                                         │
│  Claude/AI                  MCP Server                   Altium Files   │
│    │                           │                              │         │
│    │  list_package_types       │                              │         │
│    ├──────────────────────────►│                              │         │
│    │◄──────────────────────────┤  ["CHIP", "SOIC", "QFN"...]  │         │
│    │                           │                              │         │
│    │  calculate_footprint      │                              │         │
│    │  { type: "CHIP",          │                              │         │
│    │    L: 1.6, W: 0.8 }       │                              │         │
│    ├──────────────────────────►│                              │         │
│    │                           │  IPC-7351B algorithm         │         │
│    │◄──────────────────────────┤  { pads, courtyard, ... }    │         │
│    │                           │                              │         │
│    │  create_component         │                              │         │
│    │  { footprint, symbol,     │                              │         │
│    │    parameters }           │                              │         │
│    ├──────────────────────────►│                              │         │
│    │                           │  Write to .PcbLib/.SchLib    │         │
│    │                           ├─────────────────────────────►│         │
│    │◄──────────────────────────┤  { success: true }           │         │
│    │                           │                              │         │
└─────────────────────────────────────────────────────────────────────────┘
```

### IPC-7351B Compliance

All footprints follow IPC-7351B standards:

| Property | Description |
|----------|-------------|
| Land patterns | Calculated from package dimensions using IPC formulas |
| Density levels | M (Most), N (Nominal), L (Least) material conditions |
| Naming | IPC-compliant names (e.g., `CHIP_0603_N`) |
| Courtyard | Proper keepout area for assembly |
| Documentation | Assembly and silkscreen layers |

---

## Key Design Principles

1. **IPC-7351B First** — All calculations follow the standard
2. **Style Extraction** — Learn from existing library components
3. **Native Altium Files** — Read/write .PcbLib, .SchLib, .DbLib directly
4. **Version Control** — CSV database for parameters, binary files for geometry
5. **Validation** — Verify generated components against rules

---

## Supported Package Types

### Phase 1 (Initial Release)

| Family | Package Types |
|--------|---------------|
| Chip | 0201, 0402, 0603, 0805, 1206, 1210, 2512 |
| SOIC | SOIC-8, SOIC-14, SOIC-16, SOIC-20 |
| QFN | QFN-16, QFN-20, QFN-24, QFN-32, QFN-48 |
| SOT | SOT-23, SOT-223, SOT-363 |

### Phase 2 (Planned)

| Family | Package Types |
|--------|---------------|
| QFP | TQFP, LQFP (various pin counts) |
| BGA | Standard and fine-pitch BGAs |
| Through-hole | DIP, TO-220, connectors |

---

## Configuration

The MCP server is configured via JSON:

```json
{
    "library_path": "/path/to/libraries",
    "ipc": {
        "default_density": "N",
        "thermal_vias": true,
        "courtyard_margin": 0.25
    },
    "style": {
        "silkscreen_line_width": 0.15,
        "assembly_line_width": 0.10,
        "pin1_marker_style": "dot"
    }
}
```

This allows organisations to enforce consistent component styles across all libraries.

---

## MCP Tools

| Tool | Description |
|------|-------------|
| `list_package_types` | List supported IPC-7351B package families |
| `calculate_footprint` | Calculate land pattern from dimensions |
| `get_ipc_name` | Generate IPC-7351B compliant name |
| `read_pcblib` | Read components from .PcbLib file |
| `write_pcblib` | Write components to .PcbLib file |
| `create_component` | Create complete component (footprint + symbol) |
| `extract_style` | Extract style from existing library |
| `apply_style` | Apply style to components |
| `validate_component` | Verify component against IPC rules |
