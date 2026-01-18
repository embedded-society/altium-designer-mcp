# PcbLib Binary Format

This document describes the binary format of Altium Designer `.PcbLib` (PCB footprint library) files.

## File Structure

PcbLib files are OLE Compound Documents (CFB format) containing:

```text
/
├── FileHeader          # Library metadata
├── Storage             # Additional storage info
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

| ID | Type | Description |
|----|------|-------------|
| `0x01` | Arc | Arc or circle |
| `0x02` | Pad | SMD or through-hole pad |
| `0x03` | Via | Via (similar to pad) |
| `0x04` | Track | Line segment |
| `0x05` | Text | Text string |
| `0x06` | Fill | Filled rectangle |
| `0x0B` | Region | Filled polygon |
| `0x0C` | ComponentBody | 3D body reference |

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

1. Designator string block
2. Unknown (empty)
3. Marker string (`|&|0`)
4. Unknown (empty)
5. Geometry data
6. Per-layer data (optional)

**Geometry block structure:**

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1-12 | 12 | Flags and padding |
| 13-16 | 4 | X position (internal units) |
| 17-20 | 4 | Y position |
| 21-24 | 4 | Width (top) |
| 25-28 | 4 | Height (top) |
| 29-36 | 8 | Width/Height (middle) |
| 37-44 | 8 | Width/Height (bottom) |
| 45-48 | 4 | Hole size |
| 49 | 1 | Shape (top) |
| 50 | 1 | Shape (middle) |
| 51 | 1 | Shape (bottom) |
| 52-59 | 8 | Rotation (double) |
| 60 | 1 | Is plated |

**Pad shapes:**

| ID | Shape |
|----|-------|
| 1 | Round / Rounded Rectangle |
| 2 | Rectangle |
| 3 | Octagon (Oval) |

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
| 1 | 1 | Flags |
| 2 | 1 | More flags |
| 3-12 | 10 | Padding (0xFF) |

### Text (0x05)

Text has 2 blocks:

**Block 0 (Geometry):**

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1-12 | 12 | Flags and padding |
| 13-16 | 4 | X position (internal units) |
| 17-20 | 4 | Y position |
| 21-24 | 4 | Height |
| 25-26 | 2 | Font style flags |
| 27-34 | 8 | Rotation (double, degrees) |
| 35+ | var | Font name and additional data |

**Block 1 (Content):**

Length-prefixed string with text content, or reference to `WideStrings` stream.

Special text values:

| Value | Meaning |
|-------|---------|
| `.Designator` | Pad/component designator |
| `.Comment` | Component comment |

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

**Block 0 (Properties):**

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1-12 | 12 | Flags and padding |
| 13-17 | 5 | Unknown |
| 18-21 | 4 | Parameter string length |
| 22+ | var | Parameter string (ASCII key=value) |
| 22+len | 4 | Vertex count |
| 26+len | 16×N | Vertices (N pairs of doubles) |

**Block 1:** Usually empty.

**Vertex format:**

| Offset | Size | Field |
|--------|------|-------|
| 0-7 | 8 | X coordinate (double, internal units) |
| 8-15 | 8 | Y coordinate (double, internal units) |

### ComponentBody (0x0C)

3D model reference with 3 blocks:

**Block 0 (Properties):**

Contains pipe-delimited key=value parameters:

| Key | Description |
|-----|-------------|
| `V7_LAYER` | Layer (e.g., "MECHANICAL6") |
| `MODELID` | Model GUID |
| `MODEL.NAME` | Model filename |
| `MODEL.EMBED` | TRUE if embedded |
| `MODEL.3D.ROTX` | X rotation (degrees) |
| `MODEL.3D.ROTY` | Y rotation (degrees) |
| `MODEL.3D.ROTZ` | Z rotation (degrees) |
| `MODEL.3D.DZ` | Z offset (e.g., "15.748mil") |
| `STANDOFFHEIGHT` | Standoff height |
| `OVERALLHEIGHT` | Overall height |

**Block 1 and 2:** Usually empty.

### 3D Model Storage

Altium embeds 3D models in the library file:

```text
/Library/Models/
├── Header          # 4 bytes
├── Data            # Model references/metadata
├── 0               # First embedded model (STEP data)
├── 1               # Second embedded model
└── ...
```

## References

- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) - Python library for reading Altium files
- [AltiumSharp](https://github.com/issus/AltiumSharp) - C# library for Altium files
