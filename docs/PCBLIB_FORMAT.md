# PcbLib Binary Format

This document describes the binary format of Altium Designer `.PcbLib` (PCB footprint library) files.

> **Legend:** Items marked with `TODO` need implementation. Items marked with `UNKNOWN` are not fully understood.

## File Structure

PcbLib files are OLE Compound Documents (CFB format) containing:

```text
/
├── FileHeader          # Library metadata
├── Storage             # Additional storage info (UNKNOWN: purpose unclear)
├── WideStrings         # UTF-16 encoded text content (TODO: not parsed)
└── {ComponentName}/    # One storage per footprint
    └── Data            # Binary primitives stream
```

## FileHeader Stream

The FileHeader contains library-level metadata as pipe-delimited key=value pairs:

```text
[length:4 LE][text...]
```

Key fields:

| Key | Description |
|-----|-------------|
| `HEADER` | File type identifier |
| `CompCount` | Number of components |
| `LibRef{N}` | Component name (0-indexed) |
| `CompDescr{N}` | Component description |

## Data Stream Format

Each component's Data stream contains the footprint primitives:

```text
[name_block_len:4][str_len:1][name:str_len]  # Component name block
[record_type:1][blocks...]                   # First primitive
[record_type:1][blocks...]                   # Second primitive
...
[0x00]                                       # End marker
```

### Record Types

| ID | Type | Status | Description |
|----|------|--------|-------------|
| `0x01` | Arc | ✓ | Arc or circle |
| `0x02` | Pad | ✓ | SMD or through-hole pad |
| `0x03` | Via | TODO | Via (similar to pad structure) |
| `0x04` | Track | ✓ | Line segment |
| `0x05` | Text | Partial | Text string (WideStrings TODO) |
| `0x06` | Fill | ✓ | Filled rectangle |
| `0x0B` | Region | ✓ | Filled polygon |
| `0x0C` | ComponentBody | ✓ | 3D body reference |

> **Note:** Other record types may exist but have not been observed in sample files.

### Block Format

Most primitives use length-prefixed blocks:

```text
[block_length:4 LE][block_data:block_length]
```

## Coordinate System

- Altium uses internal units: **10000 units = 1 mil = 0.0254 mm**
- Conversion: `mm = internal_units / 10000.0 * 0.0254`
- Reverse: `internal_units = mm / 0.0254 * 10000.0`

## Layer IDs

### Copper and Signal Layers

| ID | Layer |
|----|-------|
| 1 | Top Layer |
| 32 | Bottom Layer |
| 74 | Multi-Layer |

### Silkscreen and Mask

| ID | Layer |
|----|-------|
| 33 | Top Overlay (Silkscreen) |
| 34 | Bottom Overlay |
| 35 | Top Paste |
| 36 | Bottom Paste |
| 37 | Top Solder Mask |
| 38 | Bottom Solder Mask |

### Component Layer Pairs

These mechanical layers are configured as component layer pairs in the sample library:

| ID | Mechanical | Purpose |
|----|------------|---------|
| 58 | M2 | Top Assembly |
| 59 | M3 | Bottom Assembly |
| 60 | M4 | Top Courtyard |
| 61 | M5 | Bottom Courtyard |
| 62 | M6 | Top 3D Body |
| 63 | M7 | Bottom 3D Body |

**AI assistants should prefer these dedicated layers over generic mechanical layers.**

### Other Layers

| ID | Layer |
|----|-------|
| 56 | Keep-Out Layer |
| 57 | Mechanical 1 |
| 64-72 | Mechanical 8-16 |

## Primitive Formats

### Pad (0x02)

Pads have 6 blocks:

1. Designator string block (length-prefixed)
2. UNKNOWN (typically empty)
3. Marker string (`|&|0`) — UNKNOWN purpose
4. UNKNOWN (typically empty)
5. Geometry data (main pad definition)
6. Per-layer data (optional, for complex pads)

**Geometry block structure:**

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1-12 | 12 | Flags and padding (UNKNOWN: detailed meaning) |
| 13-16 | 4 | X position (internal units, signed) |
| 17-20 | 4 | Y position (internal units, signed) |
| 21-24 | 4 | Width (top layer stack) |
| 25-28 | 4 | Height (top layer stack) |
| 29-32 | 4 | Width (middle layer stack) |
| 33-36 | 4 | Height (middle layer stack) |
| 37-40 | 4 | Width (bottom layer stack) |
| 41-44 | 4 | Height (bottom layer stack) |
| 45-48 | 4 | Hole size (0 for SMD pads) |
| 49 | 1 | Shape (top) |
| 50 | 1 | Shape (middle) |
| 51 | 1 | Shape (bottom) |
| 52-59 | 8 | Rotation (IEEE 754 double, degrees) |
| 60 | 1 | Is plated (UNKNOWN: exact encoding) |
| 61+ | var | UNKNOWN: additional pad properties |

**Pad shapes:**

| ID | Shape | Notes |
|----|-------|-------|
| 1 | Round | Also used for rounded rectangle in some contexts |
| 2 | Rectangle | Sharp corners |
| 3 | Octagon/Oval | Mapped to Oval |
| other | RoundedRectangle | Default fallback |

> **UNKNOWN:** The relationship between shape ID 1 and corner radius for rounded rectangles is not fully understood. Block 6 (per-layer data) format is not documented.

### Track (0x04)

Single block with geometry:

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1-12 | 12 | Flags and padding |
| 13-16 | 4 | Start X |
| 17-20 | 4 | Start Y |
| 21-24 | 4 | End X |
| 25-28 | 4 | End Y |
| 29-32 | 4 | Width |

### Arc (0x01)

Single block with geometry:

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1-12 | 12 | Flags and padding |
| 13-16 | 4 | Center X |
| 17-20 | 4 | Center Y |
| 21-24 | 4 | Radius |
| 25-32 | 8 | Start angle (double, degrees) |
| 33-40 | 8 | End angle (double, degrees) |
| 41-44 | 4 | Width |

## Common Header (13 bytes)

All primitives start with a common header:

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1 | 1 | Flags (UNKNOWN: bit meanings) |
| 2 | 1 | More flags (UNKNOWN: bit meanings) |
| 3-12 | 10 | Padding (typically 0xFF, UNKNOWN: may contain data) |

> **UNKNOWN:** The exact meaning of flag bytes at offsets 1-2 is not fully understood. They may contain component-locked, keepout, or net-related flags.

### Text (0x05)

Text has 2 blocks:

**Block 0 (Geometry):**

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1-12 | 12 | Flags and padding (UNKNOWN) |
| 13-16 | 4 | X position (internal units) |
| 17-20 | 4 | Y position |
| 21-24 | 4 | Height |
| 25-26 | 2 | Font style flags (UNKNOWN: bit meanings) |
| 27-34 | 8 | Rotation (double, degrees) |
| 35+ | var | Font name and additional data (UNKNOWN: format) |

**Block 1 (Content):**

Length-prefixed string with text content, or index reference to `WideStrings` stream.

**Special text values (inline):**

| Value | Meaning |
|-------|---------|
| `.Designator` | Pad/component designator |
| `.Comment` | Component comment |

**WideStrings stream format (TODO: not fully implemented):**

```text
|ENCODEDTEXT0=84,69,83,84|ENCODEDTEXT1=...|
```

Where `84,69,83,84` are ASCII codes (e.g., "TEST" = 84,69,83,84).

> **TODO:** Full WideStrings parsing is not implemented. Currently only `.Designator` and `.Comment` inline text is detected.

### Fill (0x06)

Filled rectangle, single block:

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1-12 | 12 | Flags and padding |
| 13-16 | 4 | X1 (first corner) |
| 17-20 | 4 | Y1 |
| 21-24 | 4 | X2 (second corner) |
| 25-28 | 4 | Y2 |
| 29-36 | 8 | Rotation (double, degrees) |

### Region (0x0B)

Filled polygon with 2 blocks:

**Block 0 (Properties + Vertices):**

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1-12 | 12 | Flags and padding (UNKNOWN) |
| 13-17 | 5 | UNKNOWN |
| 18-21 | 4 | Parameter string length |
| 22+ | var | Parameter string (ASCII key=value, pipe-delimited) |
| 22+len | 4 | Vertex count |
| 26+len | 16×N | Vertices (N pairs of doubles) |

**Block 1:** Usually empty (UNKNOWN purpose).

**Vertex format:**

| Offset | Size | Field |
|--------|------|-------|
| 0-7 | 8 | X coordinate (IEEE 754 double, internal units) |
| 8-15 | 8 | Y coordinate (IEEE 754 double, internal units) |

> **Note:** Vertices stored as doubles, not integers. Convert to mm: `mm = internal_units / 10000.0 * 0.0254`

### ComponentBody (0x0C)

3D model reference with 3 blocks:

**Block 0 (Properties):**

Binary header followed by pipe-delimited key=value parameters. Parameters start with `V7_LAYER=`.

| Key | Description | Example |
|-----|-------------|---------|
| `V7_LAYER` | Layer name | "MECHANICAL6" |
| `MODELID` | Model GUID | "{GUID}" |
| `MODEL.NAME` | Model filename | "RESC1005X04L.step" |
| `MODEL.EMBED` | Embedded flag | "TRUE" |
| `MODEL.3D.ROTX` | X rotation (degrees) | "0.000" |
| `MODEL.3D.ROTY` | Y rotation (degrees) | "0.000" |
| `MODEL.3D.ROTZ` | Z rotation (degrees) | "0.000" |
| `MODEL.3D.DZ` | Z offset | "15.748mil" |
| `STANDOFFHEIGHT` | Standoff height | "0mil" |
| `OVERALLHEIGHT` | Overall height | "0.4mm" |

> **Note:** Height values can be in "mil" or "mm" units.

**Block 1 and 2:** Usually empty (UNKNOWN purpose).

**V7_LAYER mapping:**

| V7_LAYER | Layer |
|----------|-------|
| MECHANICAL2 | Top Assembly |
| MECHANICAL3 | Bottom Assembly |
| MECHANICAL4 | Top Courtyard |
| MECHANICAL5 | Bottom Courtyard |
| MECHANICAL6 | Top 3D Body |
| MECHANICAL7 | Bottom 3D Body |

### 3D Model Storage

Altium embeds 3D models in the library file:

```text
/Library/Models/
├── Header          # 4 bytes (UNKNOWN format)
├── Data            # Model references/metadata (UNKNOWN format)
├── 0               # First embedded model (STEP data)
├── 1               # Second embedded model
└── ...
```

> **TODO:** 3D model storage parsing is not implemented. Models are referenced by GUID in `ComponentBody` records.

## Known Limitations

The following features are not fully understood or implemented:

| Feature | Status |
|---------|--------|
| Via primitive (0x03) | TODO: Similar to Pad, needs implementation |
| WideStrings stream | TODO: Contains UTF-16 encoded text |
| 3D model embedding | TODO: Header/Data stream format unknown |
| Per-layer pad data | UNKNOWN: Block 6 format |
| Pad corner radius | UNKNOWN: How rounded rectangle radius is stored |
| Net information | UNKNOWN: How net assignment is encoded |
| Component variants | UNKNOWN: Not observed in samples |

## References

- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) - Python library for reading Altium files
- [AltiumSharp](https://github.com/issus/AltiumSharp) - C# library for Altium files
