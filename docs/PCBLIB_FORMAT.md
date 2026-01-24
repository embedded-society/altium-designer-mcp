# PcbLib Binary Format

This document describes the binary format of Altium Designer `.PcbLib` (PCB footprint library) files.

> **Note:** This documentation is based on reverse engineering from AltiumSharp, pyAltiumLib, and sample file analysis.
> See [References](#references) for links.

## File Structure

PcbLib files are OLE Compound Documents (CFB format) containing:

```text
/
├── FileHeader          # Library metadata
├── Storage             # Additional storage info (contains UniqueIdPrimitiveInformation mappings)
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
| `0x03` | Via | ✓ | Via (6 blocks, similar to pad) |
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

### Copper Layers

| ID | Layer |
|----|-------|
| 0 | No Layer |
| 1 | Top Layer |
| 2-31 | Mid Layer 1-30 (internal copper) |
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

### Internal Plane Layers

| ID | Layer |
|----|-------|
| 39-54 | Internal Plane 1-16 |

### Mechanical and Documentation Layers

| ID | Layer |
|----|-------|
| 55 | Drill Guide |
| 56 | Keep-Out Layer |
| 57-72 | Mechanical 1-16 |
| 73 | Drill Drawing |

### Component Layer Pairs

These mechanical layers are typically configured as component layer pairs:

| ID | Mechanical | Purpose |
|----|------------|---------|
| 58 | M2 | Top Assembly |
| 59 | M3 | Bottom Assembly |
| 60 | M4 | Top Courtyard |
| 61 | M5 | Bottom Courtyard |
| 62 | M6 | Top 3D Body |
| 63 | M7 | Bottom 3D Body |

**AI assistants should prefer these dedicated layers over generic mechanical layers.**

### Special Layers

| ID | Layer |
|----|-------|
| 75 | Connect Layer |
| 76 | Background Layer |
| 77 | DRC Error Layer |
| 78 | Highlight Layer |
| 79 | Grid Color 1 |
| 80 | Grid Color 10 |
| 81 | Pad Hole Layer |
| 82 | Via Hole Layer |
| 83 | Top Pad Master |
| 84 | Bottom Pad Master |
| 85 | DRC Detail Layer |
| 255 | Unknown |

## Primitive Formats

### Pad (0x02)

Pads have 6 blocks:

1. Designator string block (length-prefixed)
2. Layer stack data (typically empty for simple pads)
3. Marker string (`|&|0`) — internal reference marker
4. Net/connectivity data (typically empty in libraries)
5. Geometry data (main pad definition)
6. Per-layer data (for complex pads with different shapes/sizes per layer)

**Geometry block structure:**

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1-12 | 12 | Flags and padding (see Common Header) |
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
| 60 | 1 | Is plated (0 = no, 1 = yes) |
| 61 | 1 | Reserved |
| 62 | 1 | Stack mode (see below) |
| 63+ | var | Additional pad properties (paste/solder mask expansion, etc.) |

**Pad shapes:**

| ID | Shape | Notes |
|----|-------|-------|
| 1 | Round | Circular pad |
| 2 | Rectangular | Sharp corners |
| 3 | Octagonal | 8-sided (often mapped to Oval) |
| 9 | RoundedRectangle | Rectangular with rounded corners |

**Stack modes:**

| ID | Mode | Description |
|----|------|-------------|
| 0 | Simple | All layers mirror top layer settings |
| 1 | TopMiddleBottom | Independent top, middle (layers 1-30), bottom |
| 2 | FullStack | Complete per-layer customization (32 layers) |

**Per-layer data (Block 6):**

When stack mode is not Simple, Block 6 contains per-layer arrays:

- 32 size entries (CoordPoint, 8 bytes each)
- 32 shape entries (1 byte each)
- 32 corner radius percentages (1 byte each, 0-100)
- 32 offset-from-hole-center entries (CoordPoint, 8 bytes each)

> **Note:** Corner radius is stored as a percentage (0-100) of the smaller pad dimension, not as an absolute value.

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
| 1 | 2 | Flags (uint16, see PcbFlags below) |
| 3-12 | 10 | Padding (typically 0xFF) |

**PcbFlags (uint16):**

| Bit | Flag | Description |
|-----|------|-------------|
| 0x0001 | Locked | Primitive is locked |
| 0x0002 | Polygon | Part of polygon pour |
| 0x0004 | KeepOut | Keep-out region |
| 0x0008 | TentingTop | Tented on top |
| 0x0010 | TentingBottom | Tented on bottom |

> **Note:** Most flags are typically 0x00 for library components. Net-related flags are used in board files.

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
| 25-26 | 2 | Stroke font ID (see below) |
| 27-34 | 8 | Rotation (double, degrees) |
| 35+ | var | Font name and additional data |

**Text kinds:**

| ID | Kind | Description |
|----|------|-------------|
| 0 | Stroke | Vector-based outline text |
| 1 | TrueType | Font-based rendering |
| 2 | BarCode | Barcode representation |

**Stroke font IDs:**

| ID | Font |
|----|------|
| 0 | Default |
| 1 | Sans Serif |
| 3 | Serif |

**Text justification:**

| ID | Position |
|----|----------|
| 1-3 | Bottom Right, Center, Left |
| 4-6 | Middle Right, Center, Left |
| 7-9 | Top Right, Center, Left |

**Block 1 (Content):**

Length-prefixed string with text content, or index reference to `WideStrings` stream via `WideStringsIndex`.

**Special text values (inline):**

| Value | Meaning |
|-------|---------|
| `.Designator` | Pad/component designator |
| `.Comment` | Component comment |

**WideStrings stream format:**

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
| 1-12 | 12 | Flags and padding |
| 13-17 | 5 | Reserved |
| 18-21 | 4 | Parameter string length |
| 22+ | var | Parameter string (ASCII key=value, pipe-delimited) |
| 22+len | 4 | Vertex count |
| 26+len | 16×N | Vertices (N pairs of doubles) |

**Block 1:** Outline data for display (usually empty in simple regions).

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
| `MODEL.EMBED` | Embedded flag | "TRUE" or "FALSE" |
| `MODEL.CHECKSUM` | Model integrity hash | Integer value |
| `MODEL.2D.X` | 2D placement X | Coordinate |
| `MODEL.2D.Y` | 2D placement Y | Coordinate |
| `MODEL.2D.ROTATION` | 2D rotation | Degrees |
| `MODEL.3D.ROTX` | X rotation (degrees) | "0.000" |
| `MODEL.3D.ROTY` | Y rotation (degrees) | "0.000" |
| `MODEL.3D.ROTZ` | Z rotation (degrees) | "0.000" |
| `MODEL.3D.DZ` | Z offset | "15.748mil" |
| `MODEL.SNAPCOUNT` | Snap point count | Integer |
| `STANDOFFHEIGHT` | Standoff height | "0mil" |
| `OVERALLHEIGHT` | Overall height | "0.4mm" |

> **Note:** Height values can be in "mil" or "mm" units.

**Block 1:** Model snap points data (usually empty).

**Block 2:** Reserved (usually empty).

**V7_LAYER mapping:**

| V7_LAYER | Layer |
|----------|-------|
| MECHANICAL2 | Top Assembly |
| MECHANICAL3 | Bottom Assembly |
| MECHANICAL4 | Top Courtyard |
| MECHANICAL5 | Bottom Courtyard |
| MECHANICAL6 | Top 3D Body |
| MECHANICAL7 | Bottom 3D Body |

### Via (0x03)

Vias have 6 blocks, similar to Pads:

1. Designator/name block (typically empty)
2. Layer stack data
3. Marker string
4. Net/connectivity data
5. Geometry data
6. Per-layer data

**Geometry block structure:**

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID (typically Multi-Layer) |
| 1-12 | 12 | Flags and padding |
| 13-16 | 4 | X position |
| 17-20 | 4 | Y position |
| 21-24 | 4 | Diameter |
| 25-28 | 4 | Hole size |
| 29 | 1 | From layer ID |
| 30 | 1 | To layer ID |
| 31-34 | 4 | Thermal relief air gap width |
| 35 | 1 | Thermal relief conductors count |
| 36-39 | 4 | Thermal relief conductors width |
| 40-43 | 4 | Solder mask expansion |
| 44 | 1 | Solder mask expansion manual flag |
| 45 | 1 | Diameter stack mode |
| 46+ | var | Per-layer diameters (32 × 4 bytes when FullStack) |

**Diameter stack mode:** Same as Pad stack modes (Simple, TopMiddleBottom, FullStack).

### 3D Model Storage

Altium embeds 3D models in the library file:

```text
/Library/Models/
├── Header          # Model count and metadata
├── Data            # Model references indexed by GUID
├── 0               # First embedded model (zlib-compressed STEP)
├── 1               # Second embedded model
└── ...
```

> **Note:** STEP models are stored with zlib compression. Models are referenced by GUID in `ComponentBody` records.

## Known Limitations

The following features are not fully understood or implemented:

| Feature | Status |
|---------|--------|
| WideStrings stream | TODO: Format known, parsing not implemented |
| 3D model embedding | TODO: zlib-compressed STEP, parsing not implemented |
| Pad hole shapes | Documented: Round(0), Square(1), Slot(2) |
| Net information | Used in board files, not library files |
| Component variants | Not applicable to library files |

## References

- [AltiumSharp](https://github.com/issus/AltiumSharp) - C# library for Altium files (MIT)
- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) - Python library for reading Altium files
- [python-altium](https://github.com/vadmium/python-altium) - Altium format documentation
- Sample analysis: `scripts/analyze_pcblib.py`
