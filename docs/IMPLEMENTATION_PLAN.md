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
| ComponentBody (0x0C) | ❌ | ❌ | Skipped |

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

## Phase 4: ComponentBody (0x0C) — 3D Models

**Priority: Low** — Complex, requires model storage handling.

### What We Know

- Uses 3 blocks
- References embedded STEP models in `/Library/Models/N`
- Models stored as zlib-compressed ASCII STEP
- Model metadata in `/Library/Models/Data`

### Architecture

```text
ComponentBody Record (in footprint Data stream)
       ↓
  References model by index
       ↓
/Library/Models/N (embedded STEP, zlib compressed)
       ↓
/Library/Models/Data (model metadata)
```

### Binary Format (needs verification)

**Block 1 (properties):**

```text
[common_header:13]     # Layer (usually M6/M7 for 3D body)
[model_index:4 u32]    # Index into /Library/Models
[x_offset:8 f64]       # X offset in mm
[y_offset:8 f64]       # Y offset
[z_offset:8 f64]       # Z offset
[rotation:8 f64]       # Rotation
```

**Block 2 & 3:** Unknown, possibly additional transforms or metadata.

### Implementation Steps

1. **Read `/Library/Models/Data`** — Parse model metadata
2. **Read `/Library/Models/N`** — Decompress zlib, extract STEP
3. **Parse ComponentBody record** — Link to model by index
4. **For writing** — Compress STEP, write to Models storage
5. **Update Footprint struct** — Store extracted model path or data

### Complexity

This is the most complex primitive because:

- Requires reading from multiple OLE streams (not just footprint Data)
- Needs zlib decompression
- Model data is large (10+ KB per model)
- Writing requires embedding STEP files

### Suggested Approach

**Read-only first:** Implement reading to extract model references and paths. Writing embedded models can be deferred.

---

## Development Order

| Phase | Primitive | Effort | Value |
|-------|-----------|--------|-------|
| 1 | Text (0x05) | Medium | High (designators) |
| 2 | Fill (0x06) | Low | Low |
| 3 | Region (0x0B) | Medium | High (courtyards) |
| 4 | ComponentBody (0x0C) | High | Medium (3D preview) |

**Recommended order:** Text → Region → Fill → ComponentBody

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
