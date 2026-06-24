# PcbLib Binary Format

This document describes the binary format of Altium Designer `.PcbLib` (PCB footprint library) files.

> **Note:** This documentation is based on reverse engineering from AltiumSharp, pyAltiumLib, and sample file analysis.
> See [References](#references) for links.

## File Structure

PcbLib files are OLE Compound Documents (CFB format, **OLE v3 with 512-byte sectors**) containing:

```text
/
├── FileHeader                          # Binary version string
├── Library/
│   ├── Header                          # u32: record count (always 1)
│   ├── Data                            # Params block + component count + names
│   └── Models/
│       ├── Header                      # u32: embedded model count
│       ├── Data                        # Model parameter records
│       ├── 0                           # First embedded model (zlib-compressed STEP)
│       ├── 1                           # Second embedded model
│       └── ...
└── {ComponentName}/                    # One storage per footprint
    ├── Header                          # u32: exact primitive count
    ├── Parameters                      # Block-prefixed pipe-delimited params
    ├── Data                            # Name block + primitives + end marker
    ├── WideStrings                     # Block-prefixed ENCODEDTEXT params
    └── UniqueIdPrimitiveInformation/
        ├── Header                      # u32: record count
        └── Data                        # Block-prefixed UID records
```

> **Note:** OLE version MUST be V3 (512-byte sectors). Altium Designer rejects V4 (4096-byte) files.

## Encoding Primitives (Building Blocks)

All Altium streams use these common encoding patterns:

| Pattern | Format |
|---------|--------|
| `WriteBlock(data)` | `[block_len:4 LE][data]` |
| `WriteStringBlock(str)` | `[block_len:4][str_len:1][raw_string]` |
| `WriteParameters(params)` | `WriteCString(pipe-params)` = `"\|KEY=VAL\|..." + \x00` |
| `WriteBlock(WriteParameters)` | `[block_len:4]["\|KEY=VAL\|..." + \x00]` |

**Critical rules:**

- Parameters ALWAYS start with a leading `|` pipe character
- Parameters ALWAYS end with `\x00` null terminator
- Block lengths INCLUDE the null terminator
- All strings use Windows-1252 encoding, NOT UTF-8

## `FileHeader` Stream

The `FileHeader` is a **53-byte** binary record with **three** fields (matching AltiumSharp's
`PcbLibWriter.WriteFileHeader`). It is **NOT** pipe-delimited key=value pairs. Altium Designer
rejects the library if the version double or the `UniqueId` block are missing.

```text
[string_length:4 LE u32]  = 27
[string_length:1 byte]    = 27
[string_data:27 bytes]    = "PCB 6.0 Binary Library File"   # version-string block (32 bytes)
[version_double:8 LE f64] = 5.01                            # raw 8 bytes, NO length prefix
[uid_length:4 LE u32]     = 8
[uid_length:1 byte]       = 8
[uid_data:8 bytes]        = "XXXXXXXX"                      # 8-char UniqueId block (13 bytes)
```

Within each string block the 4-byte and 1-byte lengths are the SAME value (redundant).
**Total: 32 + 8 + 13 = 53 bytes.**

> **Note:** Component metadata (names, descriptions) is stored in `/Library/Data`, not in `FileHeader`.

## `/Library/Data` Stream

Contains library-level parameters and the component directory:

```text
[block_len:4]["|KIND=Protel_Advanced_PCB_Library|VERSION=3.00|..." + \x00]
[component_count:4 LE u32]
// For each component:
[block_len:4][str_len:1][component_name]   // WriteStringBlock
```

The parameter block uses the standard `WriteBlock(WriteParameters)` encoding.

## Per-Component Streams

### `/{component}/Header`

```text
[primitive_count:4 LE u32]
```

This is the **exact** primitive count. NOT count + 1.

### `/{component}/Parameters`

```text
[block_len:4]["|PATTERN=MyComponent|DESCRIPTION=My Desc|" + \x00]
```

Standard `WriteBlock(WriteParameters)` encoding — block-prefixed, pipe-delimited, null-terminated.

### `/{component}/WideStrings`

```text
[block_len:4]["|ENCODEDTEXT0=72,101,108,108,111|..." + \x00]
```

When empty (no text content): `[block_len:4]["|" + \x00]` (`block_len` = 2).

### `/{component}/UniqueIdPrimitiveInformation`

**Header:** `[record_count:4 LE u32]`

**Data:** Block-prefixed records, one per primitive:

```text
[block_len:4]["|PRIMITIVEINDEX=0|PRIMITIVEOBJECTID=Pad|UNIQUEID=ABCD1234" + \x00]
```

| Field | Description |
|-------|-------------|
| `PRIMITIVEINDEX` | 0-based index within primitive type |
| `PRIMITIVEOBJECTID` | Primitive type name (Pad, Via, Track, Arc, Region, Text, Fill, ComponentBody) |
| `UNIQUEID` | 8-character alphanumeric identifier |

> **Note:** `PRIMITIVEINDEX` is 0-based (matching AltiumSharp convention). The reader auto-detects
> 0-based vs 1-based indexing for backward compatibility with older Altium files.
> Lookup is by (type, index) tuple.

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
| `0x03` | Via | Via (6 blocks, similar to pad) |
| `0x04` | Track | Line segment |
| `0x05` | Text | Text string |
| `0x06` | Fill | Filled rectangle |
| `0x0B` | Region | Filled polygon |
| `0x0C` | ComponentBody | 3D body reference |

> **Note:** Other record types may exist but have not been observed in sample files.

### Block Format

Most primitives use length-prefixed blocks:

```text
[block_length:4 LE][block_data:block_length]
```

**Block size limits:**

| Block Type | Maximum Size |
|------------|--------------|
| Standard block | 100,000 bytes |
| UniqueID record | 10,000 bytes |
| String field | 255 bytes |

**Minimum block sizes (for parsing):**

| Primitive | Minimum Size | Notes |
|-----------|--------------|-------|
| Pad geometry | 52 bytes | Required for shape data |
| Track | 33 bytes | Header + coordinates + width |
| Arc | 45 bytes | Header + geometry + angles + width |
| Text geometry | 25 bytes | Minimum to extract position; writer pads to 80 |
| Via geometry | 31 bytes | Reader minimum; writer pads to 46 |
| Fill | 37 bytes | Header + coordinates + rotation |
| Region properties | 22 bytes | Before parameter string |
| Per-layer data | 320 bytes | Minimum without offsets; 576 with offsets |

## Coordinate System

- Altium uses internal units: **10000 units = 1 mil = 0.0254 mm**
- Conversion: `mm = internal_units / 10000.0 * 0.0254`
- Reverse: `internal_units = mm / 0.0254 * 10000.0`

**Conversion constants:**

| Constant | Value | Formula |
|----------|-------|---------|
| MM_TO_INTERNAL | 393700.787... | 10000.0 / 0.0254 |
| INTERNAL_TO_MM | 2.54e-6 | 0.0254 / 10000.0 |

> **Note:** Results are typically rounded to 6 decimal places (1nm resolution) to avoid floating-point noise.

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
| 186-201 | Mechanical 17-32 (Altium Designer 18+) |

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

> The layer field is a plain `u8`. There is no `255 = Unknown` layer — any out-of-range id is
> simply unrecognised (an unknown layer *name* maps to `0 = NoLayer`).

## Primitive Formats

### Pad (0x02)

Pads have 6 blocks:

| Block | Content |
|-------|---------|
| 0 | Designator string (length-prefixed) |
| 1 | Layer stack data (typically empty for simple pads) |
| 2 | Marker string (`\|&\|0`) — internal reference marker |
| 3 | Net/connectivity data (typically empty in libraries) |
| 4 | Geometry data (main pad definition) |
| 5 | Per-layer data (for complex pads with different shapes/sizes per layer) |

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
| 61 | 1 | Hole shape (0 = Round, 1 = Square, 2 = Slot) |
| 62 | 1 | Stack mode (see below) |
| 63-85 | 23 | Reserved (zeros) |
| 86-89 | 4 | Paste mask expansion (internal units, signed) |
| 90-93 | 4 | Solder mask expansion (internal units, signed) |
| 94-100 | 7 | Reserved |
| 101 | 1 | Paste mask expansion manual (0 = auto, 1 = manual) |
| 102 | 1 | Solder mask expansion manual (0 = auto, 1 = manual) |
| 103-109 | 7 | Reserved |
| 110-111 | 2 | Jumper ID (internal use) |

**Parsing thresholds:**

| Field | Threshold | Behaviour |
|-------|-----------|-----------|
| Hole size | < 0.001 mm | Treated as zero (SMD pad) |
| Mask expansion | < 0.0001 mm | Filtered out (no manual expansion) |

**Pad shapes:**

| ID | Shape | Notes |
|----|-------|-------|
| 0 | RoundedRectangle | Default for unknown IDs |
| 1 | Round / RoundedRectangle | Distinguished by corner_radius_percent (0 or 100 = Round, 1-99 = RoundedRectangle) |
| 2 | Rectangular | Sharp corners |
| 3 | Octagonal | 8-sided (often mapped to Oval) |

> **Note:** Round and RoundedRectangle share shape ID 1. The distinction is made via the per-layer
> corner_radius_percent field. RoundedRectangle pads require FullStack mode to preserve their shape.
>
> **Implementation note:** When writing pads with `corner_radius_percent` set (1-99%) or shape
> `RoundedRectangle`, the tool automatically upgrades from Simple to FullStack mode to preserve the
> corner radius data.

**Stack modes:**

| ID | Mode | Description |
|----|------|-------------|
| 0 | Simple | All layers mirror top layer settings |
| 1 | TopMiddleBottom | Independent top, middle (layers 1-30), bottom |
| 2 | FullStack | Complete per-layer customisation (32 layers) |

**Per-layer data (Block 6):**

When stack mode is not Simple, Block 6 contains per-layer arrays:

| Offset | Size | Field |
|--------|------|-------|
| 0-255 | 256 | 32 size entries (width/height pairs, 4+4 bytes each as i32) |
| 256-287 | 32 | 32 shape IDs (1 byte each) |
| 288-319 | 32 | 32 corner radius percentages (1 byte each, 0-100) |
| 320-575 | 256 | 32 offset entries (x/y pairs, 4+4 bytes each as i32) — optional |

**Layer index mapping:**

| Index | Layer |
|-------|-------|
| 0 | Top Layer |
| 1 | Bottom Layer |
| 2-31 | Mid Layers 1-30 |

Total size: 320 bytes minimum (without offsets), 576 bytes with offsets.

> **Note:** Corner radius is stored as a percentage (0-100) of the smaller pad dimension, not as an absolute value.
> Default corner radius for RoundedRectangle is 50% if not specified.

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
| 1-2 | 2 | Flags word (uint16, see below) |
| 3-4 | 2 | NetIndex (uint16; `0xFFFF` when unconnected) |
| 5-6 | 2 | PolygonIndex (uint16; `0xFFFF`) |
| 7-8 | 2 | ComponentIndex (uint16; `0xFFFF`) |
| 9-12 | 4 | Reserved (uint32) |

For a footprint-library primitive the index fields are unused (all `0xFF`), which is why bytes
3–12 look like padding — but in a board file they carry the net / polygon / component links.

**Flags word (bytes 1–2, uint16) — on-wire bits** (`src/altium/pcblib/flags.rs`):

| Bit | Flag | Meaning |
|-----|------|---------|
| `0x0004` | Unlocked | **Inverted** — set = unlocked, clear = locked |
| `0x0008` | Saved | Set on a saved primitive |
| `0x0020` | TentingTop | Solder-mask tented on top |
| `0x0040` | TentingBottom | Solder-mask tented on bottom |
| `0x0200` | Keepout | Keep-out primitive |

> **Note:** These are the literal bits on disk. The crate also exposes an internal abstract
> `PcbFlags` enum (`LOCKED` / `KEEPOUT` / `TENTING_TOP` / `TENTING_BOTTOM`, with its own distinct
> values) for the public API; the reader/writer translate between the two. Do not confuse the
> abstract enum's values with the on-wire bits above.

### Text (0x05)

Text has 2 blocks:

**Block 0 (Geometry):**

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1 | 1 | Text kind (0=Stroke, 1=TrueType, 2=BarCode) |
| 2-12 | 11 | Flags and padding (0xFF) |
| 13-16 | 4 | X position (internal units) |
| 17-20 | 4 | Y position |
| 21-24 | 4 | Height |
| 25-26 | 2 | Stroke font ID (see below) |
| 27-34 | 8 | Rotation (double, degrees) |
| 35-38 | 4 | Font size (same as height) |
| 39-42 | 4 | Reserved (zeros) |
| 43-60 | 18 | Font name (UTF-16LE, null-terminated, e.g., "Arial") |
| 61-67 | 7 | Font style bytes |
| 68-71 | 4 | Line spacing |
| 72 | 1 | Justification (see below) |
| 73 | 1 | Reserved |
| 74-77 | 4 | Glyph width |
| 78-114 | 37 | Reserved padding |
| 115-116 | 2 | WideStringsIndex (u16, reference to WideStrings stream) |
| 117+ | var | Additional padding |

**Block size constraints:**

| Constraint | Value | Notes |
|------------|-------|-------|
| Minimum for reading | 25 bytes | Minimum to extract basic geometry |
| Writer padding | 80 bytes | Total block padded to at least 80 bytes |

> **Note:** WideStrings index at offset 115 references the WideStrings stream.
> Justification offset (67-72) may vary based on font name length.
>
> Font style bytes at offset 61-67 are typically: `0x56, 0x40, 0x01, 0x00, 0x00, 0x00, 0x00`.

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
| 0 | Bottom Left |
| 1 | Bottom Center |
| 2 | Bottom Right |
| 3 | Middle Left |
| 4 | Middle Center |
| 5 | Middle Right |
| 6 | Top Left |
| 7 | Top Center |
| 8 | Top Right |

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

> **Note:** Inline `.Designator` and `.Comment` text is detected. Full WideStrings stream parsing is documented above.
>
> WideStrings indices: Index 0 is reserved for `.Designator`, index 1 for `.Comment`. Actual text content starts at index 2.

### Fill (0x06)

Filled rectangle, single block (50 bytes total):

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1-12 | 12 | Flags and padding |
| 13-16 | 4 | X1 (first corner) |
| 17-20 | 4 | Y1 |
| 21-24 | 4 | X2 (second corner) |
| 25-28 | 4 | Y2 |
| 29-36 | 8 | Rotation (double, degrees) |
| 37-49 | 13 | Reserved (zeros) |

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

**Parameter string format:**

```text
V7_LAYER={layer}|NAME= |KIND=0|...
```

| Key | Description |
|-----|-------------|
| `V7_LAYER` | Layer name (e.g., "TOPLAYER") |
| `NAME` | Region name (usually empty) |
| `KIND` | Region kind (0 = standard) |

> **Note:** Layer names are formatted by removing spaces and converting to uppercase (e.g., "Top Layer" → "TOPLAYER").

**Block 1:** Outline data for display (usually empty in simple regions).

**Vertex format:**

| Offset | Size | Field |
|--------|------|-------|
| 0-7 | 8 | X coordinate (IEEE 754 double, internal units) |
| 8-15 | 8 | Y coordinate (IEEE 754 double, internal units) |

> **Note:** Vertices are stored as doubles but rounded to integers before conversion:
> `mm = round(internal_double) / 10000.0 * 0.0254`

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
| `ARCRESOLUTION` | Arc resolution | "0.5mil" |
| `ISSHAPEBASED` | Shape-based flag | "FALSE" |
| `CAVITYHEIGHT` | Cavity height | "0mil" |
| `BODYPROJECTION` | Body projection | "0" |
| `BODYCOLOR3D` | 3D body colour | "8421504" |
| `BODYOPACITY3D` | 3D body opacity | "1.000" |
| `MODEL.MODELTYPE` | Model type | "1" |
| `MODEL.MODELSOURCE` | Model source | "Undefined" |

> **Note:** Height values can be in "mil" or "mm" units. The tool parses both formats:
> `15.748mil` → 0.4mm, `0.4mm` → 0.4mm. Mil values are converted using factor 0.0254 (1 mil = 0.0254 mm).

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

| Block | Content |
|-------|---------|
| 0 | Designator/name block (typically empty) |
| 1 | Layer stack data |
| 2 | Marker string (`\|&\|0`) |
| 3 | Net/connectivity data |
| 4 | Geometry data |
| 5 | Per-layer diameters (when stack mode ≠ Simple) |

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
| 31-34 | 4 | Thermal relief air gap width (default: 10 mils = 2540 units) |
| 35 | 1 | Thermal relief conductors count (default: 4) |
| 36-39 | 4 | Thermal relief conductors width (default: 10 mils = 2540 units) |
| 40-43 | 4 | Solder mask expansion |
| 44 | 1 | Solder mask expansion manual flag |
| 45 | 1 | Diameter stack mode |
| 46+ | var | Per-layer diameters (32 × 4 bytes when FullStack) |

**Diameter stack mode:** Same as Pad stack modes (Simple, TopMiddleBottom, FullStack).

Via geometry block minimum size: 46 bytes.

**Per-layer diameters (Block 6):**

When diameter stack mode is not Simple, Block 6 contains 32 diameter values (4 bytes each as i32), using the same layer index mapping as Pads.

### 3D Model Storage

Altium embeds 3D models in the library file:

```text
/Library/Models/
├── Header          # 4-byte LE u32 model count
├── Data            # Length-prefixed model records
├── 0               # First embedded model (zlib-compressed STEP)
├── 1               # Second embedded model
└── ...
```

**Header stream format:**

A single 4-byte little-endian unsigned integer containing the number of embedded models.

**Data stream format:**

A sequence of block-prefixed records using standard `WriteBlock(WriteParameters)` encoding:

```text
[block_len:4 LE]["|EMBED=TRUE|MODELSOURCE=Undefined|ID={GUID}|..." + \x00]
[block_len:4 LE]["|EMBED=TRUE|..." + \x00]
...
```

Block length includes the null terminator. Parameters start with a leading `|`.

Each record contains pipe-delimited key=value pairs:

| Field | Description |
|-------|-------------|
| `EMBED` | `TRUE` if model is embedded |
| `MODELSOURCE` | Model source (usually `Undefined`) |
| `ID` | GUID matching `MODELID` in ComponentBody |
| `ROTX`, `ROTY`, `ROTZ` | Rotation values (degrees) |
| `DZ` | Z offset |
| `CHECKSUM` | Model checksum |
| `NAME` | Model filename (e.g., `model.step`) |

The record's position (0, 1, 2, ...) corresponds to the model stream index.

> **Note:** STEP models are stored with zlib compression. Models are referenced by GUID in `ComponentBody` records.

## Primitive Writing Order

When writing footprint data, primitives are encoded in this specific order:

1. Arcs (0x01)
2. Pads (0x02)
3. Vias (0x03)
4. Tracks (0x04)
5. Text (0x05)
6. Regions (0x0B)
7. Fills (0x06)
8. ComponentBodies (0x0C)
9. End marker (0x00)

## Notes

- **OLE version**: MUST be V3 (512-byte sectors); Altium rejects V4
- **WideStrings stream**: Per-component, block-prefixed; inline `.Designator` and `.Comment` are detected
- **3D model embedding**: zlib-compressed STEP files, referenced by GUID
- **Pad hole shapes**: Round (0), Square (1), Slot (2)
- **Net information**: Used in board files, not library files
- **Component variants**: Not applicable to library files
- **Unique IDs**: All primitives support 8-character alphanumeric unique IDs for tracking (0-based index)
- **Default layer mapping**: Unknown layer IDs default to Multi-Layer (74)
- **Default hole shape**: Unknown hole shape IDs default to Round (0)
- **Default stack mode**: Unknown stack mode IDs default to Simple (0)
- **Default justification**: Unknown justification IDs default to MiddleCenter (4)
- **Internal OLE entries filtered**: FileHeader, Library, Models, Textures, ModelsNoEmbed, PadViaLibrary, LayerKindMapping, ComponentParamsTOC, FileVersionInfo, PrimitiveGuids

## References

- [AltiumSharp](https://github.com/issus/AltiumSharp) - C# library for Altium files (MIT)
- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) - Python library for reading Altium files
- [python-altium](https://github.com/vadmium/python-altium) - Altium format documentation
- Sample analysis: `scripts/analyse/analyse_pcblib.py`
