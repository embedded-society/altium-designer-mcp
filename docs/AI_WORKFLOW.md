# AI Workflow Guide

This document explains how an AI assistant uses altium-designer-mcp to create Altium components.

## Core Principle

**The AI handles the intelligence. The tool handles file I/O.**

See [VISION.md](VISION.md) for the full responsibility split and architectural rationale.

---

## The Complete Workflow

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                    AI's Component Creation Workflow                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. UNDERSTAND THE REQUEST                                                  │
│     Engineer: "Create a 0603 chip resistor footprint"                       │
│     AI reasons about the component requirements                             │
│                                                                             │
│  2. CALCULATE DIMENSIONS (AI's job)                                         │
│     AI applies IPC-7351B formulas:                                          │
│     • Body: 1.6mm × 0.8mm                                                   │
│     • Terminal length: 0.3mm                                                │
│     • Pad size = terminal + toe + heel                                      │
│     • Courtyard = body + margins                                            │
│                                                                             │
│  3. DEFINE PRIMITIVES (AI's job)                                            │
│     AI constructs the complete footprint:                                   │
│     • Pads with exact positions and sizes                                   │
│     • Silkscreen tracks                                                     │
│     • Courtyard region                                                      │
│     • Assembly outline                                                      │
│                                                                             │
│  4. WRITE TO FILE (Tool's job)                                              │
│     AI calls: write_pcblib { filepath, footprints }                         │
│     Tool writes the OLE compound document                                   │
│                                                                             │
│  5. VERIFY                                                                  │
│     AI calls: read_pcblib to verify the result                              │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Step-by-Step Example: Creating a 0603 Resistor

### 1. AI Calculates the Footprint

The AI applies IPC-7351B knowledge:

```text
Component: 0603 Chip Resistor (1608 metric)
Body: 1.6mm × 0.8mm × 0.55mm
Terminal: 0.3mm

IPC-7351B Calculations (Nominal density):
- Toe extension: 0.35mm
- Heel extension: 0.35mm
- Side extension: 0.05mm

Pad dimensions (using manufacturer-recommended values):
- Width: 0.9mm (terminal + extensions, adjusted for process)
- Height: 0.95mm (body height + side extensions, adjusted)
- Centre-to-centre span: 1.5mm

IPC Name: RESC1608X55N
```

### 2. AI Constructs Primitives

```json
{
    "name": "RESC1608X55N",
    "description": "Chip resistor, 0603 (1608 metric), IPC-7351B Nominal",
    "pads": [
        {
            "designator": "1",
            "x": -0.75,
            "y": 0,
            "width": 0.9,
            "height": 0.95,
            "shape": "rounded_rectangle",
            "layer": "Top Layer"
        },
        {
            "designator": "2",
            "x": 0.75,
            "y": 0,
            "width": 0.9,
            "height": 0.95,
            "shape": "rounded_rectangle",
            "layer": "Top Layer"
        }
    ],
    "tracks": [
        { "x1": -0.8, "y1": -0.55, "x2": 0.8, "y2": -0.55, "width": 0.12, "layer": "Top Overlay" },
        { "x1": -0.8, "y1": 0.55, "x2": 0.8, "y2": 0.55, "width": 0.12, "layer": "Top Overlay" }
    ],
    "regions": [
        {
            "vertices": [
                { "x": -1.45, "y": -0.73 },
                { "x": 1.45, "y": -0.73 },
                { "x": 1.45, "y": 0.73 },
                { "x": -1.45, "y": 0.73 }
            ],
            "layer": "Top Courtyard"
        }
    ]
}
```

### 3. AI Calls write_pcblib

**MCP Tool Call:**

```json
{
    "name": "write_pcblib",
    "arguments": {
        "filepath": "./Passives.PcbLib",
        "footprints": [
            {
                "name": "RESC1608X55N",
                "description": "Chip resistor, 0603 (1608 metric)",
                "pads": [...],
                "tracks": [...],
                "regions": [...]
            }
        ]
    }
}
```

**Response:**

```json
{
    "success": true,
    "footprints_written": 1
}
```

### 4. AI Verifies the Result

**MCP Tool Call:**

```json
{
    "name": "read_pcblib",
    "arguments": {
        "filepath": "./Passives.PcbLib"
    }
}
```

---

## Available MCP Tools

### read_pcblib

Read footprints from an existing library. Coordinates are in millimetres.

```json
{
    "name": "read_pcblib",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib"
    }
}
```

**For large libraries**, use pagination to avoid output limits:

```json
{
    "name": "read_pcblib",
    "arguments": {
        "filepath": "./LargeLibrary.PcbLib",
        "component_name": "RESC1608X55N"
    }
}
```

| Parameter | Description |
|-----------|-------------|
| `component_name` | Fetch only this specific footprint |
| `limit` | Maximum footprints to return |
| `offset` | Skip first N footprints |

### write_pcblib

Write footprints with complete primitive definitions.

```json
{
    "name": "write_pcblib",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib",
        "footprints": [...]
    }
}
```

### read_schlib

Read symbols from an existing schematic library. Coordinates are in schematic units (10 = 1 grid).

```json
{
    "name": "read_schlib",
    "arguments": {
        "filepath": "./MyLibrary.SchLib"
    }
}
```

Pagination parameters (`component_name`, `limit`, `offset`) work the same as `read_pcblib`.

### write_schlib

Write symbols with complete primitive definitions.

```json
{
    "name": "write_schlib",
    "arguments": {
        "filepath": "./MyLibrary.SchLib",
        "symbols": [...]
    }
}
```

### list_components

List component names in a library.

```json
{
    "name": "list_components",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib"
    }
}
```

### extract_style

Extract styling information from an existing library.

```json
{
    "name": "extract_style",
    "arguments": {
        "filepath": "./MyLibrary.PcbLib"
    }
}
```

Returns statistics about track widths, pad shapes, pin lengths, colours, and layer usage.

---

## Primitive Types

The AI provides complete primitive definitions. The tool writes them.

### Pad

```json
{
    "designator": "1",
    "x": 0,
    "y": 0,
    "width": 1.0,
    "height": 0.8,
    "shape": "rounded_rectangle",
    "layer": "Top Layer",
    "hole_size": null,
    "rotation": 0
}
```

### Track

```json
{
    "x1": -1.0,
    "y1": 0.5,
    "x2": 1.0,
    "y2": 0.5,
    "width": 0.12,
    "layer": "Top Overlay"
}
```

### Arc

```json
{
    "x": 0,
    "y": 0,
    "radius": 0.5,
    "start_angle": 0,
    "end_angle": 360,
    "width": 0.12,
    "layer": "Top Overlay"
}
```

### Region

```json
{
    "vertices": [
        { "x": -1, "y": -1 },
        { "x": 1, "y": -1 },
        { "x": 1, "y": 1 },
        { "x": -1, "y": 1 }
    ],
    "layer": "Top Courtyard"
}
```

### Text

```json
{
    "x": 0,
    "y": 1.5,
    "text": ".Designator",
    "height": 0.8,
    "layer": "Top Overlay",
    "rotation": 0
}
```

---

## Standard Altium Layers

| Layer | Usage |
|-------|-------|
| Top Layer | SMD copper pads |
| Multi-Layer | Through-hole pads (all copper layers) |
| Top Overlay | Silkscreen |
| Top Paste | Solder paste stencil |
| Top Solder | Solder mask openings |
| Top Assembly | Assembly outline |
| Top 3D Body | 3D body outline |
| Top Courtyard | Courtyard (IPC-7351) |

---

## Working with Large Libraries

For libraries with many components, use pagination to avoid output limits:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ LARGE LIBRARY WORKFLOW                                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. LIST COMPONENTS FIRST                                                   │
│     AI calls: list_components { filepath }                                  │
│     Returns: ["RESC0402...", "RESC0603...", ...]                            │
│                                                                             │
│  2. FETCH SPECIFIC COMPONENTS                                               │
│     AI calls: read_pcblib { filepath, component_name: "RESC0603..." }       │
│     Returns: Single footprint with full details                             │
│                                                                             │
│  3. OR PAGINATE THROUGH ALL                                                 │
│     AI calls: read_pcblib { filepath, limit: 5, offset: 0 }                 │
│     Response includes: has_more: true, total_count: 50                      │
│     AI calls: read_pcblib { filepath, limit: 5, offset: 5 }                 │
│     ... continues until has_more: false                                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Batch Component Creation

The AI can create entire libraries efficiently:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ BATCH CREATION: 100 components in minutes                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  User: "Create a resistor library with all standard chip sizes"             │
│                                                                             │
│  AI:                                                                        │
│    1. List standard chip sizes: [0201, 0402, 0603, 0805, 1206, 2512]        │
│    2. For each size:                                                        │
│       - Look up body dimensions                                             │
│       - Apply IPC-7351B formulas                                            │
│       - Construct pad, track, region primitives                             │
│    3. Call write_pcblib with all footprints                                 │
│    4. Verify with read_pcblib                                               │
│                                                                             │
│  Result: Complete resistor library created                                  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Tips for AI Assistants

### 1. Apply IPC-7351B Correctly

The AI is responsible for correct calculations:

- Use appropriate density level (M/N/L) for the application
- Calculate toe, heel, and side extensions
- Determine courtyard margins
- Generate correct IPC names

### 2. Use Consistent Style

When creating multiple components:

- Use the same silkscreen line width (typically 0.12mm or 0.15mm)
- Use the same courtyard margins
- Place silkscreen outside the pads
- Use consistent layer assignments

### 3. Verify Dimensions

Before calling write_pcblib:

- Body length > terminal length
- Pad width and height > 0
- Courtyard encompasses all pads
- Silkscreen doesn't overlap pads

### 4. Handle Through-Hole Components

For through-hole pads:

- Set `layer` to "Multi-Layer"
- Provide `hole_size` in mm
- Pad size should be hole + annular ring

---

## Error Handling

### File Not Found

```json
{
    "error": "Failed to read file: ./Missing.PcbLib"
}
```

### Invalid Primitive

```json
{
    "error": "Invalid parameter 'width': must be positive"
}
```

### Write Error

```json
{
    "error": "Failed to write file: ./Library.PcbLib"
}
```

---

## Why This Architecture?

This design lets the AI create **any footprint** — not just pre-programmed package types.

| Approach | Limitation |
|----------|------------|
| Calculator-based | Only supports coded package types |
| Primitive-based | AI can create any footprint |

The AI already knows IPC-7351B. The tool doesn't need to duplicate that knowledge.

---

## IPC Standards Reference

When calculating footprints, the AI should apply these IPC standards:

### Primary Standards

| Standard | Description | Key Content |
|----------|-------------|-------------|
| **IPC-7351B** | Generic Requirements for Surface Mount Design and Land Pattern Standard | Pad dimensions, courtyard, naming conventions |
| **IPC-2221** | Generic Standard on Printed Board Design | Through-hole annular rings, via sizing |
| **IPC-2222** | Sectional Design Standard for Rigid Organic Printed Boards | Layer stackup, design rules |

### IPC-7351B Quick Reference

**Density Levels:**

| Level | Code | Application |
|-------|------|-------------|
| Most (M) | N/A | High-density designs, fine-pitch |
| Nominal (N) | N | Standard manufacturing |
| Least (L) | N/A | Wave soldering, hand assembly |

**Naming Convention:**

```text
RESC1608X55N
│   │    │ └── Density: N=Nominal, M=Most, L=Least
│   │    └──── Height in 0.01mm (55 = 0.55mm)
│   └───────── Body size in 0.01mm (1608 = 1.6mm x 0.8mm)
└───────────── Package type (RESC = Chip Resistor)
```

**Common Package Codes:**

| Code | Package Type |
|------|-------------|
| RESC | Chip Resistor |
| CAPC | Chip Capacitor |
| INDC | Chip Inductor |
| DIOM | Molded Diode |
| LEDC | Chip LED |
| SOIC | Small Outline IC |
| QFP | Quad Flat Package |
| QFN | Quad Flat No-Lead |
| BGA | Ball Grid Array |
| SOT | Small Outline Transistor |

### Official IPC Resources

- **IPC Standards Store**: [shop.ipc.org](https://shop.ipc.org/)
- **IPC-7351B LP Calculator**: [PCB Libraries Calculator](https://www.pcblibraries.com/Products/FPL_702-IPC7351LandPatternCalculator.asp)
- **IPC-7351B Naming and Land Pattern Tool**: [PCB Libraries](https://www.pcblibraries.com/)

### Additional References

- **JEDEC Package Outlines**: [jedec.org/standards-documents](https://www.jedec.org/standards-documents)
- **EIA/JEDEC Component Sizes**: Standard chip sizes (0201, 0402, 0603, etc.)
