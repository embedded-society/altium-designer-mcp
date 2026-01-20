# TODO

This document tracks implementation gaps between the documented PcbLib format (`docs/PCBLIB_FORMAT.md`) and the Rust codebase.

## Primitives

### Via (0x03) - Implemented ✓

- [x] Add `Via` struct to `src/altium/pcblib/primitives.rs`
- [x] Implement `parse_via()` in `src/altium/pcblib/reader.rs`
- [x] Implement `encode_via()` in `src/altium/pcblib/writer.rs`
- [x] Add `vias: Vec<Via>` field to `Footprint` struct in `src/altium/pcblib/mod.rs`
- [x] Core fields: x, y, diameter, hole_size, from_layer, to_layer, solder_mask_expansion
- [x] Advanced fields: thermal relief settings (gap, conductors, width), diameter stack mode (Simple/TopMiddleBottom/FullStack), per-layer diameters

### WideStrings Stream - Implemented ✓

- [x] Parse `/WideStrings` stream during library read in `src/altium/pcblib/mod.rs`
- [x] Decode `ENCODEDTEXT{N}=...` format (comma-separated ASCII codes)
- [x] Store decoded strings in a lookup table (`WideStrings` type in reader.rs)
- [x] Update `parse_text()` in `src/altium/pcblib/reader.rs` to use WideStrings lookup
- [x] Write WideStrings stream when saving libraries
- [ ] TODO: Verify WideStringsIndex offset in geometry block (currently tries offsets 95-97)

### 3D Model Embedding - Implemented ✓

- [x] Parse `/Library/Models/Header` stream for model count
- [x] Parse `/Library/Models/Data` stream for GUID-to-index mapping
- [x] Extract zlib-compressed STEP files from `/Library/Models/{N}` streams
- [x] Add `EmbeddedModel` struct with id, name, data, compressed_size
- [x] Add `models()`, `get_model()`, `add_model()` methods to `PcbLib`
- [x] Add model embedding support when writing libraries
- [x] Link `ComponentBody.model_id` to embedded model data via `get_model()`

## Pad Advanced Features

### Hole Shapes - Implemented ✓

- [x] Add `HoleShape` enum with `Round`, `Square`, `Slot` variants to `src/altium/pcblib/primitives.rs`
- [x] Add `hole_shape: HoleShape` field to `Pad` struct
- [x] Add `hole_shape_from_id()` in `src/altium/pcblib/reader.rs` to handle IDs 0 (Round), 1 (Square), 2 (Slot)
- [x] Add `hole_shape_to_id()` in `src/altium/pcblib/writer.rs`
- [x] Parse hole shape from geometry offset 61 in `parse_pad()`
- [x] Write hole shape in `encode_pad_geometry()`
- Note: Hole shapes are separate from pad shapes (copper outline)

### Paste/Solder Mask Expansion - Implemented ✓

- [x] Add `paste_mask_expansion: Option<f64>` field to `Pad`
- [x] Add `solder_mask_expansion: Option<f64>` field to `Pad`
- [x] Add `paste_mask_expansion_manual: bool` field
- [x] Add `solder_mask_expansion_manual: bool` field
- [x] Parse from geometry block offsets 86-93 and 101-102
- [x] Write to geometry block in `encode_pad_geometry()`
- [x] Add roundtrip tests for mask expansion

### Corner Radius - Implemented ✓

- [x] Add `corner_radius_percent: Option<u8>` field to `Pad` struct (0-100%)
- [x] Parse corner radius from per-layer data (Block 5)
- [x] Write corner radius to per-layer data (Block 5, offset 288)
- [x] Set stack mode to FullStack (2) when corner radius is specified
- Note: Percentage of smaller pad dimension, not absolute value

### Stack Modes - Implemented ✓

- [x] Add `stack_mode: PadStackMode` field to `Pad` struct
- [x] Create `PadStackMode` enum: `Simple`, `TopMiddleBottom`, `FullStack`
- [x] Parse stack mode byte at geometry offset 62 in `parse_pad()` (reader.rs)
- [x] Write stack mode in `encode_pad_geometry()` (writer.rs)
- [x] Auto-upgrade to FullStack when corner_radius_percent is set

### Per-Layer Pad Data - Implemented ✓

- [x] Add per-layer size arrays to `Pad` struct (32 CoordPoints)
- [x] Add per-layer shape arrays (32 bytes)
- [x] Add per-layer corner radius percentages (32 bytes, 0-100)
- [x] Add per-layer offset-from-hole-center arrays (32 CoordPoints)
- [x] Parse Block 5 in `parse_pad()` when stack mode != Simple
- [x] Write Block 5 in `encode_pad()` when stack mode != Simple

## Text Advanced Features

### Text Kinds - Implemented ✓

- [x] Add `TextKind` enum: `Stroke`, `TrueType`, `BarCode`
- [x] Add `kind: TextKind` field to `Text` struct
- [x] Parse text kind from geometry block (offset 1)
- [x] Write text kind to geometry block (offset 1)

### Stroke Font IDs - Implemented ✓

- [x] Add `StrokeFont` enum: `Default`, `SansSerif`, `Serif`
- [x] Add `stroke_font: Option<StrokeFont>` field to `Text`
- [x] Parse stroke font ID (bytes 25-26 in geometry block)
- [x] Write stroke font ID

### Text Justification - Implemented ✓

- [x] Add `TextJustification` enum with 9 positions (BottomLeft, BottomCenter, etc.)
- [x] Add `justification: TextJustification` field to `Text`
- [x] Parse justification from geometry block (offset 67)
- [x] Write justification to geometry block

## Layers

### Mid Layers - Implemented ✓

- [x] Mid layers: `MidLayer1` through `MidLayer30` (IDs 2-31)

### Internal Planes - Implemented ✓

- [x] Internal planes: `InternalPlane1` through `InternalPlane16` (IDs 39-54)
- [x] Added to `Layer` enum in `src/altium/pcblib/primitives.rs`
- [x] Added to `layer_from_id()` in reader.rs
- [x] Added to `layer_to_id()` in writer.rs

### Drill Layers - Implemented ✓

- [x] `DrillGuide` (ID 55)
- [x] `DrillDrawing` (ID 73)

### Mechanical Layers - Implemented ✓

- [x] Mechanical 1-16 (IDs 57-72)
- [x] Mechanical 2-7 aliased to component layers (TopAssembly, BottomAssembly, etc.)

### Special Layers - Implemented ✓

- [x] `ConnectLayer` (ID 75)
- [x] `BackgroundLayer` (ID 76)
- [x] `DRCErrorLayer` (ID 77)
- [x] `HighlightLayer` (ID 78)
- [x] `GridColor1` (ID 79)
- [x] `GridColor10` (ID 80)
- [x] `PadHoleLayer` (ID 81)
- [x] `ViaHoleLayer` (ID 82)
- [x] `TopPadMaster` (ID 83)
- [x] `BottomPadMaster` (ID 84)
- [x] `DRCDetailLayer` (ID 85)

## PcbFlags - Implemented ✓

- [x] Add `PcbFlags` struct or bitflags to primitives
- [x] Add flags field to all primitives (Pad, Track, Arc, Region, Fill, Text)
- [x] Parse flags from common header bytes 1-2 (reader.rs)
- [x] Write flags to common header (writer.rs)

Flag bits supported:

- [x] `LOCKED` (0x0001)
- [x] `POLYGON` (0x0002)
- [x] `KEEPOUT` (0x0004)
- [x] `TENTING_TOP` (0x0008)
- [x] `TENTING_BOTTOM` (0x0010)

Note: Text primitive uses byte 1 for `TextKind` instead of flags, so flags are always empty for Text.

## OLE Streams - Implemented ✓

### Storage Stream

- [x] Parse `UniqueIdPrimitiveInformation` mappings from `/Storage` stream (stub implemented, logs found entries)
- [ ] Use mappings to link primitives to unique IDs (requires real files for reverse engineering)

### FileHeader Improvements

- [x] Parse `CompCount` field (number of components)
- [x] Parse `LibRef{N}` fields (component names by index)
- [x] Parse `CompDescr{N}` fields (component descriptions)
- [x] Write complete FileHeader with all fields
- [x] Add `LibraryMetadata` struct for storing parsed header data
- [x] Add `metadata()` accessor method to `PcbLib`

## Code Quality

### Error Handling - Implemented ✓

- [x] Return `Result` from `parse_pad()`, `parse_track()`, etc. instead of `Option`
- [x] Add specific error types for parse failures
- [x] Improve error messages with offset information

Note: All parse functions now return `ParseResult<T>` which is `Result<(T, usize), AltiumError>`.
Error messages include the primitive type, block number, and byte offset where parsing failed.

### Testing - Mostly Complete

- [x] Add roundtrip tests for Via primitive
- [x] Add tests for WideStrings parsing
- [x] Add tests for 3D model parsing and embedding
- [x] Add tests for pad hole shapes, mask expansion (basic features)
- [x] Add tests for advanced pad features (stack modes, per-layer data)
- [x] Add tests for all layer ID mappings
- [ ] Add integration tests with real Altium library files

Note: Tests added for `PadStackMode` (Simple, TopMiddleBottom, FullStack), per-layer pad data
(sizes, shapes, corner radii, offsets), and comprehensive layer ID mapping tests covering
all copper layers, mid layers, mask layers, internal planes, mechanical layers, and special layers.

### Documentation - Implemented ✓

- [x] Add doc comments to all public types in primitives.rs
- [x] Document coordinate system in primitives.rs module docs
- [x] Add examples to Pad, Track, Arc struct docs

Note: Module documentation includes coordinate system diagram (origin, X/Y axes, rotation direction),
layer recommendations table, and internal units explanation. Doc examples with `cargo test` verification
for Pad::smd, Pad::through_hole, Track::new, and Arc::circle.

## Low Priority / Future

- [x] Support reading/writing SchLib files (Label primitive support added, roundtrip tests pass)
- [ ] Support component variants (board-level feature, not library)
- [ ] Support net information (board-level feature, not library)
- [ ] Optimize binary parsing with zero-copy where possible
