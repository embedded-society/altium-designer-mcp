# AI Workflow Guide

This document explains how an AI assistant uses altium-designer-mcp to create Altium components.

## Core Principle

**The AI handles the intelligence. The tool handles the file I/O.**

| Responsibility | Owner |
|---------------|-------|
| IPC-7351B calculations | AI |
| Package layout decisions | AI |
| Style choices | AI |
| Datasheet interpretation | AI |
| Reading/writing Altium files | This tool |
| Primitive placement | This tool |

---

## The Complete Workflow

```
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

```
Component: 0603 Chip Resistor (1608 metric)
Body: 1.6mm × 0.8mm × 0.55mm
Terminal: 0.3mm

IPC-7351B Calculations (Nominal density):
- Toe extension: 0.35mm
- Heel extension: 0.35mm
- Side extension: 0.05mm

Pad dimensions:
- Width: 0.3 + 0.35 + 0.35 = 1.0mm (but typically 0.9mm)
- Height: 0.8 + 2×0.05 = 0.9mm (but typically 0.95mm)
- Span: 1.6 - 0.3 + 0.9 = 2.2mm (centre-to-centre: 1.5mm)

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
      "layer": "Mechanical 15"
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

Read footprints from an existing library.

```json
{
  "name": "read_pcblib",
  "arguments": {
    "filepath": "./MyLibrary.PcbLib"
  }
}
```

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
  "layer": "Mechanical 15"
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
| Mechanical 1 | Assembly outline |
| Mechanical 13 | 3D body outline |
| Mechanical 15 | Courtyard |

---

## Batch Component Creation

The AI can create entire libraries efficiently:

```
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
