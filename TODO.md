# TODO

Missing features and improvements for altium-designer-mcp.

---

## SchLib - Missing Features

### Pin Symbol Decorations

**Priority:** Medium

Pin symbols (InnerEdge, OuterEdge, Inside, Outside) are documented in `docs/SCHLIB_FORMAT.md` but not implemented.
Currently, all four symbol bytes are read/written as zeros.

**22 symbol types to implement:**

| ID | Symbol | Description |
|----|--------|-------------|
| 0 | None | No decoration |
| 1 | Dot | Inversion dot (bubble) |
| 2 | RightLeftSignalFlow | Right-to-left signal flow arrow |
| 3 | Clock | Clock input indicator |
| 4 | ActiveLowInput | Active low input bar |
| 5 | AnalogSignalIn | Analog signal input |
| 6 | NotLogicConnection | Not a logic connection |
| 7 | PostponedOutput | Postponed output |
| 8 | OpenCollector | Open collector output |
| 9 | HiZ | High impedance |
| 10 | HighCurrent | High current |
| 11 | Pulse | Pulse |
| 12 | Schmitt | Schmitt trigger input |
| 13 | ActiveLowOutput | Active low output bar |
| 14 | OpenCollectorPullUp | Open collector with pull-up |
| 15 | OpenEmitter | Open emitter output |
| 16 | OpenEmitterPullUp | Open emitter with pull-up |
| 17 | DigitalSignalIn | Digital signal input |
| 18 | ShiftLeft | Shift left |
| 19 | OpenOutput | Open output |
| 20 | LeftRightSignalFlow | Left-to-right signal flow arrow |
| 21 | BidirectionalSignalFlow | Bidirectional signal flow |

**Files to modify:**

- `src/altium/schlib/primitives.rs` — add `PinSymbol` enum
- `src/altium/schlib/primitives.rs` — add symbol fields to `Pin` struct
- `src/altium/schlib/reader.rs` — parse symbol bytes in `parse_binary_pin()`
- `src/altium/schlib/writer.rs` — encode symbol bytes in `encode_binary_pin()`

---

### Pin Colour

**Priority:** Low

Pin colour (4-byte BGR) is read but discarded, and written as black (0x000000).

**Files to modify:**

- `src/altium/schlib/primitives.rs` — add `colour` field to `Pin` struct
- `src/altium/schlib/reader.rs` — parse colour in `parse_binary_pin()`
- `src/altium/schlib/writer.rs` — encode colour in `encode_binary_pin()`

---

### GraphicallyLocked Flag

**Priority:** Low

The 0x40 bit in pin flags (GraphicallyLocked) is documented but not extracted.

**Files to modify:**

- `src/altium/schlib/primitives.rs` — add `graphically_locked` field to `Pin` struct
- `src/altium/schlib/reader.rs` — extract flag in `parse_binary_pin()`
- `src/altium/schlib/writer.rs` — encode flag in `encode_binary_pin()`

---

### Polyline Line Style Properties

**Priority:** Low

Polyline supports line style and endpoint shape properties that are parsed but not stored in the struct.

**Properties to implement:**

| Property | Type | Description |
|----------|------|-------------|
| `LineStyle` | int | 0=Solid, 1=Dashed, 2=Dotted |
| `StartLineShape` | int | Start endpoint shape (Arrow, Circle, etc.) |
| `EndLineShape` | int | End endpoint shape |
| `LineShapeSize` | int | Size of endpoint shapes |

**Files to modify:**

- `src/altium/schlib/primitives.rs` — add fields to `Polyline` struct
- `src/altium/schlib/reader.rs` — store parsed values
- `src/altium/schlib/writer.rs` — encode line style properties

---

### Label IsMirrored Property

**Priority:** Low

The `IsMirrored` property for Label primitives is documented but not implemented.

**Files to modify:**

- `src/altium/schlib/primitives.rs` — add `is_mirrored` field to `Label` struct
- `src/altium/schlib/reader.rs` — parse `IsMirrored` property
- `src/altium/schlib/writer.rs` — encode `IsMirrored` property

---

### Rectangle Transparent Property

**Priority:** Low

The `Transparent` property for Rectangle primitives is documented but not parsed or stored.

**Files to modify:**

- `src/altium/schlib/primitives.rs` — add `transparent` field to `Rectangle` struct
- `src/altium/schlib/reader.rs` — parse `Transparent` property
- `src/altium/schlib/writer.rs` — encode `Transparent` property

---

## PcbLib - Potential Improvements

### Additional Layer Support

**Priority:** Low

Some layer IDs default to `MultiLayer` when unknown. Consider adding explicit support for:

- Layers 86-255 (extended mechanical layers in newer Altium versions)

---

## Testing

### Real-World Library Testing

**Priority:** High

Test with real Altium libraries across different Altium Designer versions to ensure compatibility.

**Areas to test:**

- Libraries created in Altium Designer 20, 21, 22, 23, 24
- Libraries with complex multi-part symbols
- Libraries with embedded 3D models from different sources
- Libraries using pin symbol decorations

---

## Documentation

### API Documentation

**Priority:** Medium

- Add more rustdoc examples for public API functions
- Document edge cases and limitations

---
