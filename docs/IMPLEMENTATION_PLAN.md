# Implementation Plan: Text, Region, and 3D Model Primitives

This document outlines the plan for implementing the remaining unimplemented primitives.

---

## Current State

| Primitive | Read | Write | Status |
|-----------|------|-------|--------|
| Pad (0x02) | ✅ | ✅ | Complete |
| Track (0x04) | ✅ | ✅ | Complete |
| Arc (0x01) | ✅ | ✅ | Complete |
| Text (0x05) | ✅ | ✅ | Complete |
| Fill (0x06) | ✅ | ✅ | Complete |
| Region (0x0B) | ✅ | ✅ | Complete |
| ComponentBody (0x0C) | ✅ | ✅ | Complete |

---

## Phase 1: Text Primitive (0x05)

**Priority: High** — Text is needed for designator and comment labels.

### What We Know

- Uses 1 block
- Contains: layer, position (X, Y), font info, rotation, text content
- Text strings may be stored in `/WideStrings` container (UTF-16 encoded)
- Common header (13 bytes) followed by text-specific data

### Binary Format (needs verification)

```text
[common_header:13]     # Layer, flags, padding
[x:4 i32]              # X position (internal units)
[y:4 i32]              # Y position
[height:4 i32]         # Text height
[rotation:8 f64]       # Rotation in degrees
[font_id:4 u32]        # Font reference
[text_len:4 u32]       # String length
[text:text_len]        # ASCII or reference to WideStrings
```

### Implementation Steps

1. **Analyse sample files** — Hex dump Text records from sample.PcbLib
2. **Add Text struct** — Already exists in `primitives.rs`
3. **Implement `parse_text()`** — Parse the binary format
4. **Add to `parse_footprint()`** — Handle record type 0x05
5. **Implement `encode_text()`** — Write text records
6. **Add tests** — Round-trip tests

### Reference Code

- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) — Python implementation
- [AltiumSharp](https://github.com/issus/AltiumSharp) — C# implementation

---

## Phase 2: Fill Primitive (0x06)

**Priority: Medium** — Filled rectangles, less common than regions.

### What We Know

- Uses 1 block
- Contains: layer, corner coordinates (X1, Y1, X2, Y2)
- Simpler than Region (axis-aligned rectangle)

### Binary Format (needs verification)

```text
[common_header:13]     # Layer, flags, padding
[x1:4 i32]             # Corner 1 X
[y1:4 i32]             # Corner 1 Y
[x2:4 i32]             # Corner 2 X
[y2:4 i32]             # Corner 2 Y
```

### Implementation Steps

1. **Analyse sample files** — Find Fill records in existing libraries
2. **Add Fill struct** — Define in `primitives.rs`
3. **Implement parsing/encoding**
4. **Add tests**

---

## Phase 3: Region Primitive (0x0B)

**Priority: High** — Regions are used for courtyard, copper pour, etc.

### What We Know

- Uses 2 blocks
- Block 1: Properties (layer, flags)
- Block 2: Vertex list
- Each vertex: X, Y coordinates, optional arc angle
- Vertices form a closed polygon

### Binary Format (needs verification)

**Block 1:**

```text
[common_header:13]     # Layer, flags, padding
[vertex_count:4 u32]   # Number of vertices
```

**Block 2 (vertices):**

```text
[x:4 i32][y:4 i32][arc_angle:8 f64]  # Vertex 1
[x:4 i32][y:4 i32][arc_angle:8 f64]  # Vertex 2
...
```

### Implementation Steps

1. **Analyse sample files** — Find Region records, dump vertex data
2. **Update Region struct** — Add arc angle support to vertices
3. **Implement `parse_region()`** — Parse both blocks
4. **Implement `encode_region()`** — Write region records
5. **Add tests** — Polygon with various vertex counts

### Note on Arc Angles

Altium regions support curved edges via arc angles. An arc angle of 0 means a straight edge; non-zero creates an arc to the next vertex.

---

## Phase 4: ComponentBody (0x0C) — 3D Models ✅ Complete

**Status:** Read/Write implemented. Model metadata parsing complete.

### Architecture

```text
ComponentBody Record (in footprint Data stream)
       ↓
  References model by GUID (MODELID)
       ↓
/Library/Models/Data (model metadata: GUID, filename, rotations)
       ↓
/Library/Models/N (embedded STEP, zlib compressed)
```

### Binary Format (verified)

**Block 0 (properties):**

```text
[layer:1]              # Layer ID (62 = Top 3D Body)
[record_type:2]        # 0x0C 0x00
[ff_padding:10]        # 0xFF padding
[zeros:5]              # Zeros
[param_len:4 u32]      # Parameter string length (including null)
[param_string:N]       # Key=value pairs separated by |
[vertex_count:4 u32]   # Outline vertex count (usually 0)
[vertices...]          # Optional outline vertices
```

**Block 1 & 2:** Optional, usually empty (0 bytes).

### Key Parameters in Block 0

- `V7_LAYER=MECHANICAL6` — Layer (MECHANICAL6 = Top 3D Body)
- `MODELID={GUID}` — References model in /Library/Models/Data
- `MODEL.NAME=filename.step` — STEP filename
- `MODEL.EMBED=TRUE` — Model is embedded
- `MODEL.3D.ROTX/ROTY/ROTZ=0.000` — Rotation in degrees
- `MODEL.3D.DZ=0mil` — Z offset
- `STANDOFFHEIGHT=0mil` — Standoff height
- `OVERALLHEIGHT=15.748mil` — Overall component height

### Implementation Notes

- ComponentBody struct stores model reference (GUID, filename)
- Model data itself remains in `/Library/Models/N` streams
- Writing generates parameter string with model metadata
- Round-trip test verifies encode/decode preserves all values

---

## Development Order

| Phase | Primitive | Effort | Status |
|-------|-----------|--------|--------|
| 1 | Text (0x05) | Medium | ✅ Complete |
| 2 | Fill (0x06) | Low | ✅ Complete |
| 3 | Region (0x0B) | Medium | ✅ Complete |
| 4 | ComponentBody (0x0C) | High | ✅ Complete |

**All primitives implemented!**

---

## Research Resources

### Reference Implementations

- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) — Python, actively maintained
- [AltiumSharp](https://github.com/issus/AltiumSharp) — C#, comprehensive
- [python-altium](https://github.com/vadmium/python-altium) — Format documentation
- [matthiasbock/python-altium](https://github.com/matthiasbock/python-altium) — SVG export

### Documentation

- [pyAltiumLib File Structure](https://pyaltiumlib.readthedocs.io/latest/fileformat/FileStructure.html)
- [Altium Region Properties](https://www.altium.com/documentation/altium-designer/pcb-region-properties)

### Reverse Engineering Approach

1. Create test footprints in Altium with known primitives
2. Export and hex dump the Data stream
3. Compare with reference implementations
4. Document offsets and field types

---

## Testing Strategy

### Unit Tests

- Parse known binary → verify field values
- Encode struct → verify binary output
- Round-trip: encode → decode → compare

### Integration Tests

- Read sample.PcbLib → verify primitives extracted
- Write footprint with all primitives → read back → verify

### Manual Verification

- Open generated .PcbLib in Altium Designer
- Verify visual correctness

---

## Success Criteria

| Primitive | Read Criteria | Write Criteria |
|-----------|--------------|----------------|
| Text | Position, content, layer correct | Altium displays text correctly |
| Fill | Corners correct | Altium displays rectangle |
| Region | All vertices correct | Altium displays polygon |
| ComponentBody | Model reference extracted | Model attached in Altium |

---

*Created: 2026-01-18*
