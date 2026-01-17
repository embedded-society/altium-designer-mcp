# Architecture: altium-designer-mcp

This document describes the architecture of the MCP server.

## Core Principle: IPC-7351B Compliance

**All footprints are generated using IPC-7351B land pattern calculations.**

This ensures:

- Consistent, manufacturable designs
- Proper solder joint formation
- Compatibility with standard assembly processes

## Component Overview

```
src/
├── lib.rs                       # Library crate root
├── main.rs                      # CLI entry point
├── error.rs                     # Top-level error types
│
├── config/                      # Configuration
│   ├── mod.rs                   # Module exports
│   └── settings.rs              # Config file parsing + defaults
│
├── ipc7351/                     # IPC-7351B calculations (planned)
│   ├── mod.rs                   # Module exports
│   ├── chip.rs                  # Chip resistor/capacitor patterns
│   ├── soic.rs                  # SOIC/SOP patterns
│   ├── qfn.rs                   # QFN/DFN patterns
│   ├── qfp.rs                   # QFP/TQFP patterns
│   ├── bga.rs                   # BGA patterns
│   └── naming.rs                # IPC naming conventions
│
├── altium/                      # Altium file I/O (planned)
│   ├── mod.rs                   # Module exports
│   ├── cfb.rs                   # OLE compound file handling
│   ├── pcblib.rs                # .PcbLib reading/writing
│   ├── schlib.rs                # .SchLib reading/writing
│   └── records.rs               # Altium record types
│
├── style/                       # Style management (planned)
│   ├── mod.rs                   # Module exports
│   ├── extract.rs               # Extract style from existing lib
│   └── apply.rs                 # Apply style to components
│
├── symbols/                     # Symbol generation (planned)
│   ├── mod.rs                   # Module exports
│   └── generator.rs             # Schematic symbol creation
│
├── database/                    # CSV database (planned)
│   ├── mod.rs                   # Module exports
│   ├── schema.rs                # Database schema
│   └── csv_io.rs                # CSV reading/writing
│
└── mcp/                         # MCP server implementation
    ├── mod.rs                   # Module exports
    ├── server.rs                # JSON-RPC server + tool dispatch
    ├── protocol.rs              # MCP protocol types
    └── transport.rs             # stdio transport
```

## Data Flow: Component Creation

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ CREATE COMPONENT: From dimensions to Altium library                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  AI Assistant             MCP Server                       Altium File      │
│    │                         │                                │             │
│    │  calculate_footprint    │                                │             │
│    │  { type: "CHIP",        │                                │             │
│    │    body_length: 1.6,    │                                │             │
│    │    body_width: 0.8,     │                                │             │
│    │    terminal_length: 0.3 │                                │             │
│    │    density: "N" }       │                                │             │
│    ├────────────────────────►│                                │             │
│    │                         │                                │             │
│    │                         │  IPC-7351B calculation:        │             │
│    │                         │  - Pad dimensions              │             │
│    │                         │  - Pad positions               │             │
│    │                         │  - Courtyard                   │             │
│    │                         │  - Silkscreen                  │             │
│    │                         │                                │             │
│    │◄────────────────────────┤  { pads: [...],                │             │
│    │                         │    courtyard: {...},           │             │
│    │                         │    silkscreen: {...} }         │             │
│    │                         │                                │             │
│    │  create_component       │                                │             │
│    │  { footprint: {...},    │                                │             │
│    │    symbol: {...},       │                                │             │
│    │    parameters: {...} }  │                                │             │
│    ├────────────────────────►│                                │             │
│    │                         │                                │             │
│    │                         │  Write to library              │             │
│    │                         ├───────────────────────────────►│             │
│    │                         │                                │             │
│    │◄────────────────────────┤  { success: true,              │             │
│    │                         │    component_name: "..." }     │             │
│    │                         │                                │             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## IPC-7351B Calculation Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ IPC-7351B: Package dimensions to land pattern                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Input Dimensions                                                           │
│  ─────────────────                                                          │
│  Body Length (L)     ─┐                                                     │
│  Body Width (W)       ├──► Fabrication     ──► Pad Width                    │
│  Terminal Length (T) ─┤    Tolerance            Pad Height                  │
│  Terminal Width (t)  ─┘                         Pad Gap                     │
│                                                                             │
│  Density Level (M/N/L) ──► Solder Goals   ──► Toe Extension                 │
│                                               Heel Extension                │
│                                               Side Extension                │
│                                                                             │
│  Output Pattern                                                             │
│  ──────────────                                                             │
│  ┌─────────────────────────────────────────┐                                │
│  │              Courtyard                  │                                │
│  │  ┌───────────────────────────────────┐  │                                │
│  │  │         Silkscreen                │  │                                │
│  │  │  ┌─────┐           ┌─────┐       │  │                                │
│  │  │  │ PAD │           │ PAD │       │  │                                │
│  │  │  │  1  │           │  2  │       │  │                                │
│  │  │  └─────┘           └─────┘       │  │                                │
│  │  │                                   │  │                                │
│  │  └───────────────────────────────────┘  │                                │
│  └─────────────────────────────────────────┘                                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Altium File Format

Altium libraries use OLE Compound File Binary (CFB) format:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ .PcbLib File Structure                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  OLE Compound File                                                          │
│  ├── Root                                                                   │
│  │   ├── FileHeader                                                         │
│  │   ├── SectionKeys                                                        │
│  │   └── Library                                                            │
│  │       ├── ComponentParamsTOC                                             │
│  │       ├── Components                                                     │
│  │       │   ├── Component1/                                                │
│  │       │   │   ├── Data                    # Binary pad/track data        │
│  │       │   │   └── Parameters              # Component parameters         │
│  │       │   ├── Component2/                                                │
│  │       │   │   ├── Data                                                   │
│  │       │   │   └── Parameters                                             │
│  │       │   └── ...                                                        │
│  │       └── ...                                                            │
│  │                                                                          │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Style Extraction

The MCP server can extract style from existing libraries:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ STYLE EXTRACTION: Learn from existing components                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Existing Library              Extracted Style             New Components   │
│       │                             │                           │           │
│       │  Read components            │                           │           │
│       ├────────────────────────────►│                           │           │
│       │                             │                           │           │
│       │  Analyse patterns:          │                           │           │
│       │  - Silkscreen line width    │                           │           │
│       │  - Pad corner radius        │                           │           │
│       │  - Pin 1 marker style       │                           │           │
│       │  - Layer usage              │                           │           │
│       │  - Font settings            │                           │           │
│       │                             │                           │           │
│       │                             │  Apply to new             │           │
│       │                             ├──────────────────────────►│           │
│       │                             │                           │           │
│       │                             │  Consistent style         │           │
│       │                             │  across all components    │           │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Security Considerations

### File Access

- The MCP server only accesses paths configured in the config file
- No arbitrary file system access
- Path traversal attacks are prevented

### Input Validation

- All dimensions are validated against reasonable ranges
- Invalid inputs return clear error messages
- No code execution from user input

### Error Handling

- Internal errors are logged but not exposed to users
- Sensitive paths are sanitised in error messages
- Stack traces are only shown in debug mode

## Supported Git Providers

This is a local tool — no network access required for core functionality.

| Feature | Network Required |
|---------|-----------------|
| IPC calculations | No |
| Footprint generation | No |
| Library I/O | No |
| Style extraction | No |
| Distributor API | Yes (optional) |
