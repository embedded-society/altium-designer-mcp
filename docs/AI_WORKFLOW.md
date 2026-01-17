# AI Workflow Guide

This document explains how an AI assistant uses altium-designer-mcp to create Altium components.

## The Complete Workflow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        AI's Component Creation Workflow                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. DISCOVER TOOLS                                                          │
│     AI calls: tools/list                                                    │
│     Receives: Available MCP tools and their schemas                         │
│                                                                             │
│  2. GET PACKAGE INFO                                                        │
│     AI calls: list_package_types                                            │
│     Receives: ["CHIP", "SOIC", "QFN", "QFP", "BGA", ...]                     │
│                                                                             │
│  3. CALCULATE FOOTPRINT                                                     │
│     AI calls: calculate_footprint                                           │
│       { package_type: "CHIP",                                               │
│         body_length: 1.6, body_width: 0.8,                                  │
│         terminal_length: 0.3, density: "N" }                                │
│     Receives: { pads, courtyard, silkscreen, ipc_name }                     │
│                                                                             │
│  4. CREATE COMPONENT (when implemented)                                     │
│     AI calls: create_component                                              │
│       { footprint: {...}, symbol: {...}, parameters: {...} }                │
│     Receives: { success: true, component_name: "CHIP_0603_N" }              │
│                                                                             │
│  5. VERIFY                                                                  │
│     AI calls: validate_component { name: "CHIP_0603_N" }                    │
│     Receives: { valid: true, warnings: [] }                                 │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Step-by-Step Example

### 1. List Available Package Types

**MCP Tool Call:**

```json
{
  "name": "list_package_types",
  "arguments": {}
}
```

**Response:**

```json
{
  "package_types": [
    {
      "name": "CHIP",
      "description": "Chip resistors and capacitors (0201, 0402, 0603, etc.)",
      "required_dimensions": ["body_length", "body_width", "terminal_length"]
    },
    {
      "name": "SOIC",
      "description": "Small Outline IC packages",
      "required_dimensions": ["body_length", "body_width", "pin_count", "pitch"]
    }
  ]
}
```

### 2. Calculate Footprint

**MCP Tool Call:**

```json
{
  "name": "calculate_footprint",
  "arguments": {
    "package_type": "CHIP",
    "body_length": 1.6,
    "body_width": 0.8,
    "terminal_length": 0.3,
    "density_level": "N"
  }
}
```

**Response:**

```json
{
  "ipc_name": "CHIP_0603_N",
  "pads": [
    { "number": "1", "x": -0.75, "y": 0, "width": 0.9, "height": 0.95 },
    { "number": "2", "x": 0.75, "y": 0, "width": 0.9, "height": 0.95 }
  ],
  "courtyard": {
    "x": 0, "y": 0,
    "width": 2.4, "height": 1.2
  },
  "silkscreen": {
    "lines": [
      { "x1": -0.3, "y1": 0.6, "x2": 0.3, "y2": 0.6 },
      { "x1": -0.3, "y1": -0.6, "x2": 0.3, "y2": -0.6 }
    ]
  }
}
```

### 3. Get IPC Name

**MCP Tool Call:**

```json
{
  "name": "get_ipc_name",
  "arguments": {
    "package_type": "CHIP",
    "body_length": 1.6,
    "body_width": 0.8,
    "density_level": "N"
  }
}
```

**Response:**

```json
{
  "ipc_name": "CHIP_0603_N",
  "description": "IPC-7351B compliant name for 0603 chip component, Nominal density"
}
```

## Working with Existing Libraries

### Extract Style from Existing Library

```json
{
  "name": "extract_style",
  "arguments": {
    "library_path": "/path/to/existing/Library.PcbLib"
  }
}
```

**Response:**

```json
{
  "style": {
    "silkscreen_line_width": 0.15,
    "assembly_line_width": 0.10,
    "pad_corner_radius_percent": 25,
    "pin1_marker_style": "dot",
    "courtyard_layer": "Mechanical 15",
    "assembly_layer": "Mechanical 13"
  }
}
```

### Apply Style to New Components

```json
{
  "name": "apply_style",
  "arguments": {
    "style": { "silkscreen_line_width": 0.15 },
    "components": ["CHIP_0603_N", "CHIP_0805_N"]
  }
}
```

## Batch Component Creation

AI assistants can create multiple components efficiently:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ BATCH CREATION: 100 components in minutes                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  User: "Create a resistor library with all standard sizes"                  │
│                                                                             │
│  AI:                                                                        │
│    1. Get standard chip sizes: [0201, 0402, 0603, 0805, 1206, 2512]         │
│    2. Get density levels: [M, N, L]                                         │
│    3. For each size × density:                                              │
│       - calculate_footprint(...)                                            │
│       - create_component(...)                                               │
│    4. validate_library(...)                                                 │
│                                                                             │
│  Result: 18 components created in ~2 minutes                                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Tips for AI Assistants

### 1. Always Validate Dimensions

Before calculating footprints, verify dimensions are reasonable:

- Body length > terminal length
- Width > 0
- Reasonable metric sizes (typically 0.2mm to 50mm)

### 2. Use Appropriate Density Level

| Density | Use Case |
|---------|----------|
| M (Most) | Prototype boards, hand soldering |
| N (Nominal) | Standard production |
| L (Least) | High-density designs |

### 3. Extract Style First

When working with existing libraries:

1. Extract style from a reference component
2. Apply the same style to new components
3. This ensures visual consistency

### 4. Batch Similar Components

Group similar components together:

```
All 0603 sizes → then all 0805 → etc.
```

This is faster than random order.

### 5. Verify Before Committing

Always run validation before finalising:

```json
{
  "name": "validate_component",
  "arguments": {
    "component_name": "CHIP_0603_N",
    "checks": ["ipc_compliance", "drc", "style"]
  }
}
```

## Error Handling

### Invalid Dimensions

```json
{
  "error": {
    "code": "INVALID_DIMENSIONS",
    "message": "Terminal length (0.5mm) cannot exceed body length (0.4mm)"
  }
}
```

### Unsupported Package Type

```json
{
  "error": {
    "code": "UNSUPPORTED_PACKAGE",
    "message": "Package type 'CUSTOM' is not supported. Use list_package_types to see available types."
  }
}
```

### Library Write Error

```json
{
  "error": {
    "code": "WRITE_ERROR",
    "message": "Cannot write to library: file is read-only"
  }
}
```
