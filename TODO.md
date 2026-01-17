# Altium Designer MCP Server — Project TODO

> Primitive-based file I/O for AI-assisted component library creation.

---

## Executive Summary

**altium-designer-mcp** is an MCP server that provides file I/O and primitive placement
tools enabling AI assistants to create and manage Altium Designer component libraries.

### Core Principle

**The AI handles the intelligence. The tool handles file I/O.**

| Responsibility | Owner |
|---------------|-------|
| IPC-7351B calculations | AI |
| Package layout decisions | AI |
| Style choices | AI |
| Reading/writing Altium files | This tool |
| Primitive placement | This tool |
| STEP model attachment | This tool |

### Why This Architecture?

The previous approach encoded IPC-7351B formulas into "calculators" — duplicating
intelligence that the AI already has. This was over-engineered.

**Simpler approach:**

- Tool provides low-level primitives (pads, tracks, arcs, text)
- AI reasons about IPC standards, datasheets, and design choices
- Tool writes the result to native Altium files

This makes the codebase smaller, more flexible, and easier to maintain.

---

## Technology Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| **Language** | Rust | Single binary, memory safe, fast |
| **MCP SDK** | Custom (JSON-RPC) | Simple stdio transport |
| **Async Runtime** | Tokio | Industry standard |
| **OLE Parsing** | `cfb` crate | Altium uses MS OLE compound documents |

---

## Project Structure

```
altium-designer-mcp/
├── Cargo.toml
├── LICENSE                      # GPLv3
├── README.md
├── TODO.md                      # This file
│
├── src/
│   ├── main.rs                  # Entry point
│   ├── lib.rs                   # Library crate root
│   ├── error.rs                 # Error types
│   │
│   ├── config/                  # Configuration
│   │   ├── mod.rs
│   │   └── settings.rs
│   │
│   ├── mcp/                     # MCP protocol
│   │   ├── mod.rs
│   │   ├── server.rs            # Tool definitions & handlers
│   │   ├── protocol.rs          # JSON-RPC types
│   │   └── transport.rs         # Stdio transport
│   │
│   └── altium/                  # Altium file I/O (TODO)
│       ├── mod.rs
│       ├── ole.rs               # OLE compound document handling
│       ├── pcblib/
│       │   ├── mod.rs
│       │   ├── reader.rs        # Read .PcbLib files
│       │   ├── writer.rs        # Write .PcbLib files
│       │   └── primitives.rs    # Pad, Track, Arc, Region, Text
│       └── schlib/
│           ├── mod.rs
│           ├── reader.rs        # Read .SchLib files
│           ├── writer.rs        # Write .SchLib files
│           └── primitives.rs    # Pin, Rectangle, Line, Arc, Text
│
├── tests/
│   └── altium_roundtrip.rs      # Read→Write→Read validation
│
└── data/
    └── test_libraries/          # Sample .PcbLib/.SchLib for testing
```

---

## MCP Tools

### Library Reading

```rust
/// Read a .PcbLib file and return all footprints with their primitives
#[tool(name = "read_pcblib")]
async fn read_pcblib(filepath: String) -> PcbLibContents;

/// Read a .SchLib file and return all symbols with their primitives
#[tool(name = "read_schlib")]
async fn read_schlib(filepath: String) -> SchLibContents;

/// List component names in a library file
#[tool(name = "list_components")]
async fn list_components(filepath: String) -> Vec<String>;
```

### Library Writing

```rust
/// Write footprints to a .PcbLib file
/// The AI provides all primitive data (pads, tracks, etc.)
#[tool(name = "write_pcblib")]
async fn write_pcblib(
    filepath: String,
    footprints: Vec<FootprintDefinition>,
    append: Option<bool>,
);

/// Write symbols to a .SchLib file
#[tool(name = "write_schlib")]
async fn write_schlib(
    filepath: String,
    symbols: Vec<SymbolDefinition>,
    append: Option<bool>,
);
```

---

## Primitive Types

### Footprint Primitives (PcbLib)

| Primitive | Description |
|-----------|-------------|
| **Pad** | SMD or through-hole pad with designator, position, size, shape, layer |
| **Track** | Line segment on any layer (silkscreen, assembly, etc.) |
| **Arc** | Arc or circle on any layer |
| **Region** | Filled polygon (courtyard, copper pour) |
| **Text** | Text string with font, size, position, layer |

### Symbol Primitives (SchLib)

| Primitive | Description |
|-----------|-------------|
| **Pin** | Electrical pin with designator, name, position, orientation, type |
| **Rectangle** | Rectangle shape for IC body |
| **Line** | Line segment for symbol graphics |
| **Arc** | Arc for symbol graphics |
| **Text** | Text for labels, part info |

### Standard Altium Layers

| Layer | Usage |
|-------|-------|
| Top Layer | Copper pads (multi-layer for SMD) |
| Top Overlay | Silkscreen |
| Top Paste | Solder paste |
| Top Solder | Solder mask |
| Mechanical 1 | Assembly outline |
| Mechanical 13 | 3D body outline |
| Mechanical 15 | Courtyard |

The AI decides which layers to use based on the component requirements.

---

## STEP Model Integration

STEP models are **attached**, not generated. The tool links existing STEP files to footprints.

```json
{
  "step_model": {
    "filepath": "./3d-models/0603.step",
    "x_offset": 0,
    "y_offset": 0,
    "z_offset": 0,
    "rotation": 0
  }
}
```

For parametric 3D model generation, use a dedicated mechanical MCP server (future project).

---

## Example: AI Creates a 0603 Resistor Footprint

The AI calculates IPC-7351B dimensions and calls `write_pcblib`:

```json
{
  "filepath": "./Passives.PcbLib",
  "footprints": [{
    "name": "RESC1608X55N",
    "description": "Chip resistor, 0603 (1608 metric)",
    "pads": [
      { "designator": "1", "x": -0.75, "y": 0, "width": 0.9, "height": 0.95, "shape": "rounded_rectangle" },
      { "designator": "2", "x": 0.75, "y": 0, "width": 0.9, "height": 0.95, "shape": "rounded_rectangle" }
    ],
    "tracks": [
      { "x1": -0.8, "y1": -0.425, "x2": 0.8, "y2": -0.425, "width": 0.12, "layer": "Top Overlay" },
      { "x1": -0.8, "y1": 0.425, "x2": 0.8, "y2": 0.425, "width": 0.12, "layer": "Top Overlay" }
    ],
    "regions": [
      { "vertices": [{"x": -1.45, "y": -0.73}, {"x": 1.45, "y": -0.73}, {"x": 1.45, "y": 0.73}, {"x": -1.45, "y": 0.73}], "layer": "Mechanical 15" }
    ]
  }]
}
```

The AI:

- Looked up IPC-7351B tables for 0603 chip
- Calculated pad size, span, courtyard
- Decided on silkscreen style
- Generated the JSON

The tool:

- Wrote the data to the .PcbLib file
- Handled OLE compound document format

---

## Development Roadmap

### Phase 1: Foundation (Done)

- [x] MCP server scaffold
- [x] JSON-RPC protocol
- [x] Tool definitions
- [x] Configuration system

### Phase 2: Altium File I/O (Current Focus)

- [ ] OLE compound document parser (`cfb` crate integration)
- [ ] PcbLib reader — extract footprints and primitives
- [ ] PcbLib writer — create footprints from primitive data
- [ ] SchLib reader — extract symbols and primitives
- [ ] SchLib writer — create symbols from primitive data
- [ ] Round-trip validation tests

### Phase 3: Polish

- [ ] Error handling improvements
- [ ] Documentation
- [ ] Pre-built binaries
- [ ] Real-world testing

---

## Prior Art & Credits

### Altium File Format References

- **AltiumSharp** (C#): <https://github.com/issus/AltiumSharp>
- **pyAltiumLib** (Python): <https://github.com/ChrisHoyer/pyAltiumLib>
- **python-altium**: <https://github.com/vadmium/python-altium>

### MCP Inspiration

- **KiCad MCP Server**: <https://github.com/mixelpixx/KiCAD-MCP-Server>

---

## License

**GNU General Public License v3.0 (GPLv3)**

The tool is free software. Generated libraries are data, not code — you can use them
in any project.

---

## Project Maintainers

- **Organization**: [The Embedded Society](https://github.com/embedded-society/)
- **Repository**: [altium-designer-mcp](https://github.com/embedded-society/altium-designer-mcp)
