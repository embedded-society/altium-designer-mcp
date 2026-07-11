# PcbLib Binary Format

This document describes the binary format of Altium Designer `.PcbLib` (PCB footprint library) files
as implemented by this crate. Every offset below is verified against the byte-level
reverse-engineering campaign (goldens + the pyaltiumlib oracle) and the current reader/writer in
`src/altium/pcblib/`; where a field exists on disk but is not surfaced by our model it is explicitly
marked *unmodelled* rather than omitted.

> **Note:** Cross-referenced against AltiumSharp, pyAltiumLib and Altium-authored sample libraries.
> See [References](#references) for links.

## File Structure

PcbLib files are OLE Compound Documents (CFB format, **OLE v3 with 512-byte sectors**) containing:

```text
/
├── FileHeader                          # Binary version string (53 bytes)
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
    ├── Data                            # Name block + primitives (NO end marker)
    ├── WideStrings                     # Block-prefixed ENCODEDTEXT params
    └── UniqueIdPrimitiveInformation/
        ├── Header                      # u32: record count
        └── Data                        # Block-prefixed UID records
```

> **Note:** OLE version MUST be V3 (512-byte sectors). Altium Designer rejects V4 (4096-byte) files.
>
> Real Altium libraries also carry `SectionKeys` (root, for >31-char names), `FileVersionInfo`,
> `EmbeddedFonts`, `LayerKindMapping`, `PadViaLibrary`, `ComponentParamsTOC`, `Textures`,
> `ModelsNoEmbed` and `PrimitiveGuids` streams/storages. These are **unmodelled**: the reader
> ignores them and the writer does not emit them.

## Encoding Primitives (Building Blocks)

All Altium streams use these common encoding patterns:

| Pattern | Format |
|---------|--------|
| `WriteBlock(data)` | `[block_len:4 LE][data]` |
| `WriteStringBlock(str)` | `[block_len:4][str_len:1][raw_string]` |
| `WriteCStringParameterBlock(str)` | `[block_len:4]["\|KEY=VAL\|..." + \x00]` (length includes the null) |
| Pascal short string | `[str_len:1][raw_string]` (max 255 bytes) |

**Critical rules:**

- Parameter strings start with a leading `|` pipe character (exceptions: the Region /
  ComponentBody nested param blocks and the `Models/Data` records, which have **no** leading pipe)
- Parameter blocks ALWAYS end with `\x00`, and the block length INCLUDES the null terminator
- There is NO trailing pipe after the last `KEY=VALUE`
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

The parameter block uses the standard `WriteCStringParameterBlock` encoding. `VERSION=3.00`
requires the `V9_MASTERSTACK` + `V9_STACK_LAYER` layer-stack entries in the same block.

## Per-Component Streams

### `/{component}/Header`

```text
[primitive_count:4 LE u32]
```

This is the **exact** primitive count. NOT count + 1.

### `/{component}/Parameters`

```text
[block_len:4]["|PATTERN=MyComponent|DESCRIPTION=My Desc" + \x00]
```

Standard `WriteCStringParameterBlock` encoding. Altium-authored libraries also carry `HEIGHT`,
`ITEMGUID` and `REVISIONGUID` keys here.

### `/{component}/WideStrings`

```text
[block_len:4]["|ENCODEDTEXT0=72,101,108,108,111|..." + \x00]
```

One `|ENCODEDTEXT{n}=` entry per text primitive with real (non-special, non-empty) content, in
primitive order — a leading pipe per entry, NO trailing pipe. The value is the comma-separated
byte values of the text.

When empty (no text content): `[block_len:4][\x00]` (`block_len` = 1 — just the null terminator,
no pipe).

### `/{component}/UniqueIdPrimitiveInformation`

**Header:** `[record_count:4 LE u32]`

**Data:** Block-prefixed records, one per primitive **that carries a unique id**:

```text
[block_len:4]["|PRIMITIVEINDEX=0|PRIMITIVEOBJECTID=Pad|UNIQUEID=ABCD1234" + \x00]
```

| Field | Description |
|-------|-------------|
| `PRIMITIVEINDEX` | Single **global 0-based ordinal** over ALL primitives in Data-stream emit order |
| `PRIMITIVEOBJECTID` | Primitive type name (Pad, Via, Track, Arc, Text, Region, Fill, ComponentBody) |
| `UNIQUEID` | 8-character alphanumeric identifier |

> **Note:** `PRIMITIVEINDEX` is NOT a per-type index. Every primitive consumes an ordinal in the
> Data-stream order (Arcs, Pads, Vias, Tracks, Text, Regions, Fills, ComponentBodies) whether or
> not it has a unique id, so e.g. the first pad behind two silkscreen arcs is `PRIMITIVEINDEX=2`.
> On read the entry's `PRIMITIVEOBJECTID` must match the primitive found at that ordinal; a
> mismatch (e.g. a foreign file with a different index base) is skipped rather than mis-attached.

## Data Stream Format

Each component's Data stream contains the footprint primitives:

```text
[name_block_len:4][str_len:1][name:str_len]  # Component name block (WriteStringBlock)
[record_type:1][blocks...]                   # First primitive
[record_type:1][blocks...]                   # Second primitive
...
```

There is **NO end-of-stream marker**. Altium reads exactly the primitive count declared in the
component `Header` stream; a trailing `0x00` byte is mis-read as a record with object-id 0 and
breaks the stream (issue #68). Never emit one.

### Record Types

| ID | Type | Description |
|----|------|-------------|
| `0x01` | Arc | Arc or circle (single 60-byte block) |
| `0x02` | Pad | SMD or through-hole pad (6 blocks) |
| `0x03` | Via | Via (single 321-byte block) |
| `0x04` | Track | Line segment (single 49-byte block) |
| `0x05` | Text | Text string (2 blocks) |
| `0x06` | Fill | Filled rectangle (single 50-byte block) |
| `0x0B` | Region | Filled polygon (single variable-length block) |
| `0x0C` | ComponentBody | 3D body (single variable-length block) |

### Block Format

Every sub-block is length-prefixed:

```text
[block_length:4 LE][block_data:block_length]
```

**Block size limits (this crate):**

| Block Type | Maximum Size |
|------------|--------------|
| Standard block | 100,000 bytes |
| UniqueID record | 10,000 bytes |
| String field | 255 bytes |

**Block sizes (reader minimum / writer output):**

| Primitive | Reader minimum | Writer output | Notes |
|-----------|----------------|---------------|-------|
| Pad geometry (Block 4) | 52 bytes | 202 bytes | Altium rejects pads with a shorter main block |
| Pad size/shape (Block 5) | — | 0, 651 or 320/576 bytes | See [Pad](#pad-0x02) |
| Track | 33 bytes | 49 bytes | Extended tail from offset 33 |
| Arc | 45 bytes | 60 bytes | Extended tail from offset 45 |
| Text geometry | 25 bytes | 252 bytes | Fixed `TEXT_SR1_TEMPLATE` layout |
| Via | 31 bytes | 321 bytes | Fixed `VIA_SR1_TEMPLATE` layout |
| Fill | 37 bytes | 50 bytes | Tail from offset 37 |
| Region | 22 bytes + params | variable | Single block incl. vertices |
| ComponentBody | — | variable | Single block incl. outline |

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

### V7 Layer IDs

Several primitives carry a derived 32-bit "v7 saved layer id" alongside the layer byte
(`v7_layer_id` in the writer, ported from AltiumSharp `V7LayerId`):

| Layer byte | V7 id |
|------------|-------|
| 1-31 (signal top/mid) | `0x01000000 + id` |
| 32 (bottom) | `0x0100FFFF` |
| 39-54 (internal plane n) | `0x01010000 + n` |
| 57-72 (mechanical n) | `0x01020000 + n` |
| 33 / 34 (overlay) | `0x01030006` / `0x01030007` |
| 35 / 36 (paste) | `0x01030008` / `0x01030009` |
| 37 / 38 (solder) | `0x0103000A` / `0x0103000B` |
| 55 / 56 / 73 | `0x0103000C` / `0x0103000D` / `0x0103000E` |
| 74 (multi-layer) + fallback | `0x0103000F` |

## Common Header (13 bytes)

All primitives start with a common header:

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID |
| 1-2 | 2 | Flags word (u16, see below) |
| 3-4 | 2 | NetIndex (u16; `0xFFFF` when unconnected) |
| 5-6 | 2 | PolygonIndex (u16; `0xFFFF` = none) |
| 7-8 | 2 | ComponentIndex (u16; `0xFFFF` = free primitive, modelled as `-1`) |
| 9-12 | 4 | Reserved (`0xFFFFFFFF`) |

For a footprint-library primitive the index fields are unused (all `0xFF`) — but in a board file
they carry the net / polygon / component links. The reader and writer model all three indices; the
from-scratch defaults (`net = 0xFFFF`, `polygon = 0xFFFF`, `component = -1`) reproduce the `0xFF`
fill byte-identically.

**Flags word (bytes 1-2, u16) — on-wire bits** (`src/altium/pcblib/flags.rs`):

| Bit | Flag | Meaning |
|-----|------|---------|
| `0x0004` | Unlocked | **Inverted** — set = unlocked, clear = locked |
| `0x0008` | Saved | Set on a saved primitive |
| `0x0020` | TentingTop | Solder-mask tented on top |
| `0x0040` | TentingBottom | Solder-mask tented on bottom |
| `0x0200` | Keepout | Keep-out primitive |

A normal saved primitive therefore carries `0x000C` (Saved + Unlocked), not `0x0000`; a keepout
primitive carries `0x020C`.

> **Note:** These are the literal bits on disk. The crate also exposes an internal abstract
> `PcbFlags` enum (`LOCKED` / `KEEPOUT` / `TENTING_TOP` / `TENTING_BOTTOM`, with its own distinct
> values) for the public API; the reader/writer translate between the two. Do not confuse the
> abstract enum's values with the on-wire bits above.

## Primitive Formats

### Pad (0x02)

Pads have 6 blocks:

| Block | Content |
|-------|---------|
| 0 | Designator string (`WriteStringBlock`) |
| 1 | Reserved — a 1-byte block containing `0x00` |
| 2 | Marker string `\|&\|0` (`WriteStringBlock`) |
| 3 | Reserved — a 1-byte block containing `0x00` |
| 4 | Main geometry block (202 bytes) |
| 5 | Size/shape block (empty, 651 bytes, or the legacy 320/576-byte per-layer form) |

**Block 4 — main geometry (202 bytes).** Offsets 0-60 are typed geometry; offsets 61-201 are the
extended tail, written by overlaying the modelled fields onto a canonical byte template
(`PAD_EXTENDED_TAIL_TEMPLATE`) captured from a standard Altium pad:

| Offset | Size | Field | Modelled |
|--------|------|-------|----------|
| 0-12 | 13 | Common header (layer, flags, net/poly/comp indices) | yes |
| 13-16 | 4 | X position (i32, internal units) | yes |
| 17-20 | 4 | Y position (i32) | yes |
| 21-24 | 4 | Width, top layer (i32) | yes |
| 25-28 | 4 | Height, top layer (i32) | yes |
| 29-32 | 4 | Width, middle layers (i32) | yes (TopMiddleBottom mode) |
| 33-36 | 4 | Height, middle layers (i32) | yes (TopMiddleBottom mode) |
| 37-40 | 4 | Width, bottom layer (i32) | yes (TopMiddleBottom mode) |
| 41-44 | 4 | Height, bottom layer (i32) | yes (TopMiddleBottom mode) |
| 45-48 | 4 | Hole size (i32; 0 for SMD) | yes |
| 49 | 1 | Shape, top | yes |
| 50 | 1 | Shape, middle | yes (TopMiddleBottom mode) |
| 51 | 1 | Shape, bottom | yes (TopMiddleBottom mode) |
| 52-59 | 8 | Rotation (f64, degrees) | yes |
| 60 | 1 | Is plated (0/1) | write-only (derived from hole presence) |
| 61 | 1 | Reserved (`0x00`) — **NOT** the hole shape (that lives in Block 5 @262) | — |
| 62 | 1 | Stack mode (0=Simple, 1=TopMiddleBottom, 2=FullStack) | yes |
| 67 | 1 | Power-plane connection style (0=Relief, 1=Direct, 2=NoConnect) | yes |
| 68-71 | 4 | Thermal-relief conductor (spoke) width (i32; default 0.254 mm) | yes |
| 72-73 | 2 | Thermal-relief spoke count (i16; default 4) | yes |
| 74-77 | 4 | Thermal-relief air gap (i32; default 0.254 mm) | yes |
| 78-81 | 4 | Power-plane relief expansion (i32; default 0.508 mm) | yes |
| 82-85 | 4 | Power-plane (anti-pad) clearance (i32; default 0.508 mm) | yes |
| 86-89 | 4 | Paste mask expansion (i32) | yes |
| 90-93 | 4 | Solder mask expansion (i32) | yes |
| 101 | 1 | Paste mask expansion mode (0=None, 1=FromRule, 2=Manual) | yes |
| 102 | 1 | Solder mask expansion mode (0=None, 1=FromRule, 2=Manual) | yes |
| 110-111 | 2 | Jumper ID (i16) | unmodelled (template) |
| 114-117 | 4 | V7 layer id (derived from the pad's layer) | yes (write) |
| 121-124 | 4 | Solder-mask cache (mirrors @90) | unmodelled (template) |
| 126-141 | 16 | Identity GUID A (fresh per pad on write) | write-only |
| 142-157 | 16 | Identity GUID B (fresh per pad on write) | write-only |
| 162-165 | 4 | Hole positive tolerance (i32; `0x7FFFFFFF` = unset) | yes |
| 166-169 | 4 | Hole negative tolerance (i32; `0x7FFFFFFF` = unset) | yes |
| 172 | 1 | Format marker (`0x1A` PcbLib / `0x12` PcbDoc) | template |
| 185 | 1 | Reserved marker (`0x03` for a standard PcbLib pad) | yes (write) |

All remaining bytes of the tail are reserved / cache / identity values replayed verbatim from the
template so the record matches Altium's 202-byte layout exactly.

**Parsing thresholds:**

| Field | Threshold | Behaviour |
|-------|-----------|-----------|
| Hole size | ≤ 0.001 mm | Treated as zero (SMD pad) |
| Mask expansion | ≤ 0.0001 mm | Read back as `None` (no manual expansion) |

**Pad shapes (bytes @49-51 and in Block 5):**

| ID | Shape | Notes |
|----|-------|-------|
| 1 | Round / Oval | An oval pad is a Round pad with width ≠ height |
| 2 | Rectangular | Sharp corners |
| 3 | Octagonal | 8-sided |
| 9 | RoundedRectangle | Written directly as id 9; requires the 651-byte Block 5 for the corner radius |

> **Note:** For backwards compatibility with older files the reader also promotes a shape-1 pad
> whose Block 5 corner radius is 1-99% to RoundedRectangle.

**Stack modes (@62):**

| ID | Mode | Description |
|----|------|-------------|
| 0 | Simple | All layers mirror top layer settings; Block 5 empty |
| 1 | TopMiddleBottom | Independent top / middle / bottom sizes+shapes, carried in the MAIN block (@29-44, @50-51); Block 5 empty |
| 2 | FullStack | Complete per-layer customisation, carried in Block 5 |

**Block 5 — size/shape block.** Three forms are written:

1. **Empty block** (`block_len = 0`) — plain Simple / TopMiddleBottom pads.
2. **Canonical 651-byte size/shape block** — a Simple pad with a non-round hole or an explicit
   corner radius. Layout:

   | Offset | Size | Field |
   |--------|------|-------|
   | 0-115 | 116 | 29 × i32 internal-layer X sizes |
   | 116-231 | 116 | 29 × i32 internal-layer Y sizes |
   | 232-260 | 29 | 29 × u8 internal-layer shape ids |
   | 261 | 1 | Reserved |
   | 262 | 1 | Hole type (0=Round, 1=Square, 2=Slot) |
   | 263-266 | 4 | Hole slot length (i32) |
   | 267-274 | 8 | Hole rotation (f64, degrees) |
   | 275-402 | 128 | 32 × i32 per-layer X offsets from hole centre |
   | 403-530 | 128 | 32 × i32 per-layer Y offsets from hole centre |
   | 531 | 1 | Has-rounded-rect flag (u8 bool) |
   | 532-563 | 32 | 32 × u8 per-layer shape ids |
   | 564-595 | 32 | 32 × u8 per-layer corner radii (%) |
   | 596-627 | 32 | Reserved (zeros) — full-stack tail starts here |
   | 628-631 | 4 | Tail entry count (i32; we write 1) |
   | 632-635 | 4 | Tail entry stride (i32 = 15) |
   | 636-650 | 15 | One 15-byte entry: layer code (4=top signal), 3 flag bytes, shape id, size X (i32), size Y (i32), corner radius % (fixed 50), trailing 0 |

   Altium NEVER emits a bare 596-byte block — every non-empty size/shape block in a golden
   `.PcbLib` is 651 (one tail entry) or 696 (four) bytes; an under-length block makes Altium
   reject the pad. Multi-entry tails (count > 1) are currently unmodelled.

3. **Legacy per-layer block (320 or 576 bytes)** — written for FullStack pads and still accepted
   on read:

   | Offset | Size | Field |
   |--------|------|-------|
   | 0-255 | 256 | 32 size entries (width/height i32 pairs) |
   | 256-287 | 32 | 32 shape IDs (1 byte each) |
   | 288-319 | 32 | 32 corner radius percentages (0-100) |
   | 320-575 | 256 | 32 offset entries (x/y i32 pairs) — optional |

   Layer index mapping: 0 = Top, 1 = Bottom, 2-31 = Mid Layers 1-30.

> **Note:** Corner radius is stored as a percentage (0-100) of the smaller pad dimension.
> Default corner radius for RoundedRectangle is 50% if not specified.

### Track (0x04)

Single 49-byte block:

| Offset | Size | Field | Modelled |
|--------|------|-------|----------|
| 0-12 | 13 | Common header | yes |
| 13-16 | 4 | Start X (i32) | yes |
| 17-20 | 4 | Start Y (i32) | yes |
| 21-24 | 4 | End X (i32) | yes |
| 25-28 | 4 | End Y (i32) | yes |
| 29-32 | 4 | Width (i32) | yes |
| 33-34 | 2 | Sub-polygon index (i16; 0 in libraries) | write-only (0) |
| 35-38 | 4 | Solder mask expansion (i32) | yes |
| 39-40 | 2 | Paste mask expansion (i16 — 2 bytes, cf. the Arc's 1-byte field) | write-only (0) |
| 41-44 | 4 | V7 layer id (u32, derived from layer) | yes (write) |
| 45 | 1 | Keepout restrictions (u8) | yes |
| 46-48 | 3 | Reserved (zeros) | — |

Every Altium-authored track carries the extended tail (offsets 33-48); the reader accepts a
33-byte legacy block and defaults the tail fields.

### Arc (0x01)

Single 60-byte block:

| Offset | Size | Field | Modelled |
|--------|------|-------|----------|
| 0-12 | 13 | Common header | yes |
| 13-16 | 4 | Centre X (i32) | yes |
| 17-20 | 4 | Centre Y (i32) | yes |
| 21-24 | 4 | Radius (i32) | yes |
| 25-32 | 8 | Start angle (f64, degrees) | yes |
| 33-40 | 8 | End angle (f64, degrees) | yes |
| 41-44 | 4 | Width (i32) | yes |
| 45-46 | 2 | Sub-polygon index (i16; 0 in libraries) | write-only (0) |
| 47-50 | 4 | Solder mask expansion (i32) | yes |
| 51 | 1 | Paste mask expansion (u8 — 1 byte, cf. the Track's 2-byte field) | write-only (0) |
| 52-55 | 4 | V7 layer id (u32, derived from layer) | yes (write) |
| 56 | 1 | Keepout restrictions (u8) | yes |
| 57-59 | 3 | Reserved (zeros) | — |

The reader accepts a 45-byte legacy block and defaults the tail fields.

### Text (0x05)

Text has 2 blocks:

- **Block 0**: the fixed **252-byte** geometry record (`TEXT_SR1_TEMPLATE`) — the writer overlays
  the modelled fields onto a canonical template captured from a real Altium text primitive and
  replays every reserved byte verbatim.
- **Block 1**: the text content as a `WriteStringBlock` (`[u32 len][u8 str_len][Windows-1252 text]`).

**Block 0 — 252-byte geometry record:**

| Offset | Size | Field | Modelled |
|--------|------|-------|----------|
| 0-12 | 13 | Common header (layer, flags, net/poly/comp indices) | yes |
| 13-16 | 4 | X position (i32) | yes |
| 17-20 | 4 | Y position (i32) | yes |
| 21-24 | 4 | Height (i32) | yes |
| 25-26 | 2 | Stroke font id (u16; 1=Default, 2=Sans Serif, 3=Serif) | yes |
| 27-34 | 8 | Rotation (f64, degrees) | yes |
| 35 | 1 | Mirrored (u8 bool) | yes |
| 36-39 | 4 | Stroke line width (i32; template default 4 mil) | yes |
| 40 | 1 | IsComment flag (u8 bool) | unmodelled (offset verified; dropped on read, 0 on write) |
| 41 | 1 | IsDesignator flag (u8 bool) | unmodelled (offset verified; dropped on read, 0 on write) |
| 42 | 1 | Character set (u8) | unmodelled (template) |
| 43 | 1 | Base font type (0=stroke, 1=TrueType) | derived from kind @160 |
| 44 | 1 | Bold (u8 bool) | yes |
| 45 | 1 | Italic (u8 bool) | yes |
| 46-109 | 64 | Font name (UTF-16LE, null-terminated; default "Arial") | yes |
| 110 | 1 | IsInverted (u8 bool) | yes |
| 111-114 | 4 | Inverted border width (i32) | yes |
| 115-118 | 4 | WideStrings index (i32; -1 = none) | partially (reader consults the low u16 @115) |
| 119-122 | 4 | Union index (i32) | unmodelled (template) |
| 123 | 1 | UseInvertedRectangle (u8 bool) | yes |
| 124-127 | 4 | Inverted rect width (i32) | yes |
| 128-131 | 4 | Inverted rect height (i32) | yes |
| 132 | 1 | Text-box justification (u8, see below) | yes |
| 133-136 | 4 | Inverted rect text offset (i32) | yes |
| 137-159 | 23 | Barcode geometry / kind / render fields | unmodelled (BarCode deferred; template) |
| 160 | 1 | **Text kind (authoritative)**: 0=Stroke, 1=TrueType, 2=BarCode | yes |
| 161-224 | 64 | Barcode font name (UTF-16LE) | unmodelled (template) |
| 225 | 1 | Barcode show-text flag | unmodelled (template) |
| 226-229 | 4 | V7 layer id (u32, derived from layer) | yes (write) |
| 230-251 | 22 | Frame / snapping / snap-point fields | unmodelled (template) |

**Text-box justification (@132)** — Altium encodes this column-major, 1-based (0 = manual):

| ID | Position | ID | Position | ID | Position |
|----|----------|----|----------|----|----------|
| 1 | Top Left | 4 | Top Centre | 7 | Top Right |
| 2 | Middle Left | 5 | Middle Centre | 8 | Middle Right |
| 3 | Bottom Left | 6 | Bottom Centre | 9 | Bottom Right |

The from-scratch default `BottomLeft` encodes to `0x03` (the template byte); ids 0 and 3 both
decode to BottomLeft.

**Text kinds (@160):**

| ID | Kind | Description |
|----|------|-------------|
| 0 | Stroke | Vector-based outline text |
| 1 | TrueType | Font-based rendering (base font type @43 = 1) |
| 2 | BarCode | Barcode representation (deferred — not modelled) |

**Block 1 (Content):** a length-prefixed string with the text content, a special string, or a
numeric WideStrings index reference.

**Special text values (inline):**

| Value | Meaning |
|-------|---------|
| `.Designator` | Pad/component designator |
| `.Comment` | Component comment |

> **Note:** When Block 1 is empty the reader falls back to scanning the geometry block for the
> special strings and to the WideStrings index at offset 115.

### Fill (0x06)

Filled rectangle, single 50-byte block:

| Offset | Size | Field | Modelled |
|--------|------|-------|----------|
| 0-12 | 13 | Common header | yes |
| 13-16 | 4 | X1 (first corner, i32) | yes |
| 17-20 | 4 | Y1 (i32) | yes |
| 21-24 | 4 | X2 (second corner, i32) | yes |
| 25-28 | 4 | Y2 (i32) | yes |
| 29-36 | 8 | Rotation (f64, degrees) | yes |
| 37-40 | 4 | Solder mask expansion (i32) | yes |
| 41 | 1 | Paste mask expansion (u8) | write-only (0) |
| 42-45 | 4 | V7 layer id (u32, derived from layer) | yes (write) |
| 46 | 1 | Keepout restrictions (u8) | yes |
| 47-49 | 3 | Reserved (zeros) | — |

### Region (0x0B)

Filled polygon — a **single** size-prefixed block. (Earlier revisions of this crate wrote a
spurious empty second block, which Altium read as an invalid record type, silently dropping every
primitive after the region.)

| Offset | Size | Field |
|--------|------|-------|
| 0-12 | 13 | Common header (layer, flags, net/poly/comp indices) |
| 13 | 1 | Reserved (`0x00`) |
| 14-15 | 2 | Hole contour count (u16) |
| 16-17 | 2 | Reserved (`0x00 0x00`) |
| 18-21 | 4 | Parameter string length (u32, INCLUDES the null terminator) |
| 22.. | var | Parameter string (Windows-1252 C-string, **no leading pipe**) |
| +0 | 4 | Outline vertex count (u32) |
| +4 | 16×N | Outline vertices: N × (f64 X, f64 Y) in internal units |
| ... | var | Hole contours: hole_count × `[u32 count][count × (f64 X, f64 Y)]` |

**Parameter string** — canonical key order (no leading pipe, `|`-separated):

```text
V7_LAYER={token}|NAME={name}|KIND={kind}|SUBPOLYINDEX={spi}|UNIONINDEX={uix}|ARCRESOLUTION={mil}|ISSHAPEBASED={bool}|CAVITYHEIGHT={mil}
```

| Key | Description | Default |
|-----|-------------|---------|
| `V7_LAYER` | Canonical layer token — `MECHANICAL{n}` for mechanical layers, else the display name upper-cased with spaces stripped (e.g. `TOPLAYER`) | matches layer byte |
| `NAME` | Region name | empty |
| `KIND` | Region kind (0 = copper/standard, 1 = cutout, ...) | 0 |
| `SUBPOLYINDEX` | Sub-polygon index | -1 |
| `UNIONINDEX` | Union index | 0 |
| `ARCRESOLUTION` | Arc resolution, mil-suffixed (`0mil`, `0.5mil`) | 0mil |
| `ISSHAPEBASED` | `TRUE` / `FALSE` | FALSE |
| `CAVITYHEIGHT` | Cavity height, mil-suffixed | 0mil |
| `NET` | Numeric net index (read-side override of the header index) | absent |

Board-region keys the model does not consume (`LAYER`, `KEEPOUT`, `ISBOARDCUTOUT`, ...) are
captured verbatim on read and re-emitted after the canonical set, so a read-modify-write does not
drop them.

> **Warning:** Altium resolves a Region's layer from the `V7_LAYER` token, NOT the header layer
> byte. Mechanical layers MUST use their `MECHANICAL{n}` token (e.g. Top Courtyard →
> `MECHANICAL4`); the space-stripped display name is not a valid token and makes Altium silently
> drop the region onto Top Layer.

**Vertex format:** each vertex is two IEEE 754 doubles (X then Y) in internal units. Values are
whole internal units in real files; the reader rounds to integers before converting to mm.

### ComponentBody (0x0C)

3D body — a **single** size-prefixed block (verified against AltiumSharp and the `BODY_3D` /
`BODY_3D_STEP` golden libraries). The outline lives inside the block; there are NO separate
snap-point or reserved blocks, and there is no `MODEL.SNAPCOUNT` parameter.

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | Layer ID (e.g. 62 for Top 3D Body) |
| 1-2 | 2 | Record-type marker (`0x0C 0x00`) |
| 3-12 | 10 | Net/polygon/component indices + reserved (`0xFF` fill for a free body) |
| 13-17 | 5 | Zeros |
| 18-21 | 4 | Parameter string length (u32, INCLUDES the null) |
| 22.. | var | Parameter string (Windows-1252 C-string, **no leading pipe**, starts `V7_LAYER=`) |
| +0 | 4 | Outline vertex count (u32) |
| +4 | 16×N | Outline vertices: N × (f64 X, f64 Y) in internal units |

> **Warning:** Outline coordinates MUST be whole internal units. Altium silently drops a body
> whose outline has fractional internal coordinates.

**Parameter keys** (writer canonical order; values from the typed model):

| Key | Description | Example |
|-----|-------------|---------|
| `V7_LAYER` | Canonical layer token (must match the header layer byte) | `MECHANICAL6` |
| `NAME` | Body name | ` ` (single space) |
| `KIND` | Body kind | `0` |
| `SUBPOLYINDEX` | Sub-polygon index | `-1` |
| `UNIONINDEX` | Union index | `0` |
| `ARCRESOLUTION` | Arc resolution (emitted TWICE — repeated after `BODYPROJECTION`, matching Altium) | `0.5mil` |
| `ISSHAPEBASED` | Shape-based flag | `FALSE` |
| `CAVITYHEIGHT` | Cavity height | `0mil` |
| `STANDOFFHEIGHT` | Standoff height (mil-suffixed, trimmed) | `0mil` |
| `OVERALLHEIGHT` | Overall height | `15.748mil` |
| `BODYPROJECTION` | Body projection | `0` |
| `BODYCOLOR3D` | 3D body colour | `8421504` |
| `BODYOPACITY3D` | 3D body opacity | `1.000` |
| `IDENTIFIER` | Identifier (comma-separated codepoint list — deferred, emitted empty) | `` |
| `TEXTURE`, `TEXTURECENTERX/Y`, `TEXTURESIZEX/Y`, `TEXTUREROTATION` | Texture fields (fixed defaults) | |
| `MODELID` | Model GUID (synthesised for extruded bodies when absent) | `{GUID}` |
| `MODEL.CHECKSUM` | Model integrity checksum (round-tripped verbatim, see below) | `0` |
| `MODEL.EMBED` | `TRUE` / `FALSE` | |
| `MODEL.NAME` | Model filename | `RESC1005X04L.step` |
| `MODEL.2D.X`, `MODEL.2D.Y` | 2D placement (fixed `0mil`) | |
| `MODEL.2D.ROTATION` | 2D rotation (degrees, 3 decimals) | `0.000` |
| `MODEL.3D.ROTX/Y/Z` | 3D rotations (degrees, 3 decimals) | `0.000` |
| `MODEL.3D.DZ` | Z offset (mil-suffixed) | `15.748mil` |
| `MODEL.MODELTYPE` | `0` = extruded, `1` = model-backed (STEP) | |
| `MODEL.EXTRUDED.MINZ` / `MAXZ` | Extrusion Z range (extruded bodies ONLY) | `0mil` / `15.748mil` |
| `MODEL.MODELSOURCE` | Model source (model-backed bodies ONLY) | `Undefined` |

**Extruded vs model-backed:** a body with no model file (empty `MODEL.NAME`, not embedded) is a
generic *extruded* body — `MODEL.MODELTYPE=0` plus `MODEL.EXTRUDED.MINZ/MAXZ` (the Z range Altium
extrudes the outline between; without it the body has no volume and is discarded). A model-backed
body uses `MODEL.MODELTYPE=1` plus `MODEL.MODELSOURCE=Undefined` instead. `ISSHAPEBASED` stays
`FALSE` in both cases.

Unmodelled keys captured on read are re-emitted verbatim after the canonical set (with canonical
duplicates dropped) so read-modify-write round-trips.

> **Note:** Height values can be in "mil" or "mm" units on read. The tool parses both formats;
> mil values are converted using 1 mil = 0.0254 mm.

**V7_LAYER mapping** (any `MECHANICAL1`-`MECHANICAL32` token is valid; the common pairs):

| V7_LAYER | Layer |
|----------|-------|
| MECHANICAL2 | Top Assembly |
| MECHANICAL3 | Bottom Assembly |
| MECHANICAL4 | Top Courtyard |
| MECHANICAL5 | Bottom Courtyard |
| MECHANICAL6 | Top 3D Body |
| MECHANICAL7 | Bottom 3D Body |

The body's authoritative layer on read is the header layer byte at offset 0; `V7_LAYER` is the
fallback only when the byte is absent.

### Via (0x03)

Altium writes a via as **ONE** block: a fixed **321-byte** record (`VIA_SR1_TEMPLATE`) whose first
13 bytes are the common header. (A 6-block pad-style layout is wrong and was misread by Altium —
issue #113.) The writer overlays the modelled fields onto the canonical template; undecoded
constant regions are replayed verbatim.

| Offset | Size | Field | Modelled |
|--------|------|-------|----------|
| 0-12 | 13 | Common header (layer byte = 74 Multi-Layer; flags; net/poly/comp indices) | yes |
| 13-16 | 4 | X position (i32) | yes |
| 17-20 | 4 | Y position (i32) | yes |
| 21-24 | 4 | Diameter (i32) | yes |
| 25-28 | 4 | Hole size (i32) | yes |
| 29 | 1 | From (start) layer ID | yes |
| 30 | 1 | To (end) layer ID | yes |
| 31 | 1 | Power-plane connection style (0=Relief, 1=Direct, 2=NoConnect) | yes |
| 32-35 | 4 | Thermal relief air gap (i32; default 0.254 mm) | yes |
| 36 | 1 | Thermal relief conductor count (u8; default 4) | yes |
| 38-41 | 4 | Thermal relief conductor width (i32; default 0.254 mm) | yes |
| 42-45 | 4 | Power-plane relief expansion (i32; default 0.508 mm) | yes |
| 46-49 | 4 | Power-plane clearance (i32; default 0.508 mm) | yes |
| 50-53 | 4 | Paste mask expansion (i32) | yes |
| 54-57 | 4 | Solder mask expansion, front (i32) | yes |
| 61-64 | 4 | Cache-valid word | unmodelled (template) |
| 66 | 1 | Solder mask expansion mode (0=None, 1=FromRule, 2=Manual) | yes |
| 74 | 1 | Diameter stack mode (0=Simple, 1=TopMiddleBottom, 2=FullStack) | yes |
| 75-202 | 128 | Per-layer diameters (32 × i32; a Simple via repeats its diameter) | yes |
| 242-245 | 4 | Solder mask expansion, back/bottom face (i32; mirrors the front when unset) | yes |
| 258 | 1 | Solder-mask-from-hole-edge flag | unmodelled (template) |
| 259-274 | 16 | Identity GUID A (fresh per via on write) | write-only |
| 275-290 | 16 | Identity GUID B (fresh per via on write) | write-only |
| 291-294 | 4 | Hole positive tolerance (i32; `0x7FFFFFFF` = unset) | yes |
| 295-298 | 4 | Hole negative tolerance (i32; `0x7FFFFFFF` = unset) | yes |
| 312 | 1 | Drill layer-pair type (0=Through, 1/2/3 = blind/buried) | unmodelled (template) |
| 320 | 1 | Trailing constant (`0x01`) | template |

The reader accepts any block ≥ 31 bytes and defaults every absent field.

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

**Header stream format:** a single 4-byte little-endian unsigned integer containing the number of
embedded models.

**Data stream format:** a sequence of C-string parameter blocks — **NO leading pipe** (the record
starts at `EMBED=`, matching AltiumSharp's `string.Join` and every `BODY_3D` golden):

```text
[block_len:4 LE]["EMBED=TRUE|MODELSOURCE=Undefined|ID={GUID}|ROTX=0.000|ROTY=0.000|ROTZ=0.000|DZ=0|CHECKSUM=0|NAME=model.step" + \x00]
...
```

Block length includes the null terminator.

| Field | Description |
|-------|-------------|
| `EMBED` | `TRUE` if model is embedded |
| `MODELSOURCE` | Model source (usually `Undefined`) |
| `ID` | GUID matching `MODELID` in ComponentBody |
| `ROTX`, `ROTY`, `ROTZ` | Rotation values (degrees) |
| `DZ` | Z offset |
| `CHECKSUM` | Model checksum (see below) |
| `NAME` | Model filename (e.g. `model.step`) |

The record's position (0, 1, 2, ...) corresponds to the numbered model stream index. The
numbered streams (`/Library/Models/0`, ...) are the raw model bytes with a standard **zlib**
wrapper (RFC 1950: `78 9C` header + Adler-32; matches flate2 `ZlibEncoder` / .NET `ZLibStream`).

**Checksum algorithm** (`PcbModel.ComputeChecksum` in AltiumSharp): a position-weighted byte sum
over the **uncompressed** model bytes — weight 1 for byte 0, weight `i` for byte `i` — modulo
2^32, stored as a signed i32. `CHECKSUM=0` is explicitly tolerated by Altium, and this crate
writes 0 rather than recomputing (the value is round-tripped verbatim on read-modify-write; a
recomputed body checksum that disagreed with the Models/Data record would be worse than 0/0).

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

There is **no** end marker after the last primitive (issue #68).

## Notes

- **OLE version**: MUST be V3 (512-byte sectors); Altium rejects V4
- **No end markers**: neither the Data stream nor any primitive carries a trailing `0x00`
- **WideStrings stream**: per-component, block-prefixed; inline `.Designator` and `.Comment` are detected
- **3D model embedding**: zlib-compressed STEP files, referenced by GUID
- **Pad hole shapes**: Round (0), Square (1), Slot (2) — stored at offset 262 of the 651-byte size/shape block, NOT in the main geometry block
- **Net information**: modelled via the common-header indices; used in board files, not library files
- **Unique IDs**: 8-character alphanumeric, keyed by a single global 0-based `PRIMITIVEINDEX` over all primitives in Data order
- **Default layer mapping**: unknown layer IDs default to Multi-Layer (74)
- **Default stack mode**: unknown stack mode IDs default to Simple (0)
- **Internal OLE entries filtered on read**: FileHeader, Library, Models, Textures, ModelsNoEmbed, PadViaLibrary, LayerKindMapping, ComponentParamsTOC, FileVersionInfo, PrimitiveGuids

## References

- [AltiumSharp](https://github.com/issus/AltiumSharp) - C# library for Altium files (MIT)
- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) - Python library for reading Altium files
- [python-altium](https://github.com/vadmium/python-altium) - Altium format documentation
- Sample analysis: `scripts/analyse/analyse_pcblib.py`
