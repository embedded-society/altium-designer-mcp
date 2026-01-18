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

### Stroke Font IDs - Not Exposed

- [ ] Add `StrokeFont` enum: `Default`, `SansSerif`, `Serif`
- [ ] Add `stroke_font: Option<StrokeFont>` field to `Text`
- [ ] Parse stroke font ID (bytes 25-26 in geometry block)
- [ ] Write stroke font ID

### Text Justification - Not Exposed

- [ ] Add `TextJustification` enum with 9 positions (BottomRight, BottomCenter, etc.)
- [ ] Add `justification: TextJustification` field to `Text`
- [ ] Parse justification from geometry block
- [ ] Write justification (currently hardcoded to 4 in writer.rs line 391)

## Layers - Incomplete Coverage

### Missing Layer Variants

Add to `Layer` enum in `src/altium/pcblib/primitives.rs`:

- [ ] Mid layers: `MidLayer1` through `MidLayer30` (IDs 2-31)
- [ ] Internal planes: `InternalPlane1` through `InternalPlane16` (IDs 39-54)
- [ ] `DrillGuide` (ID 55)
- [ ] `DrillDrawing` (ID 73)
- [ ] Mechanical 3-12, 14, 16 (IDs 59-68, 70, 72) - currently some are aliased incorrectly

### Missing Special Layers

- [ ] `ConnectLayer` (ID 75)
- [ ] `BackgroundLayer` (ID 76)
- [ ] `DRCErrorLayer` (ID 77)
- [ ] `HighlightLayer` (ID 78)
- [ ] `GridColor1` (ID 79)
- [ ] `GridColor10` (ID 80)
- [ ] `PadHoleLayer` (ID 81)
- [ ] `ViaHoleLayer` (ID 82)
- [ ] `TopPadMaster` (ID 83)
- [ ] `BottomPadMaster` (ID 84)
- [ ] `DRCDetailLayer` (ID 85)

### Layer Mapping Fixes

- [ ] Fix `layer_from_id()` in reader.rs (line 130-156) - many IDs incorrectly default to MultiLayer
- [ ] Fix `layer_to_id()` in writer.rs (line 70-94) - incomplete mapping

## PcbFlags - Not Exposed

- [ ] Add `PcbFlags` struct or bitflags to primitives
- [ ] Add flags field to all primitives (Pad, Track, Arc, Region, Fill, Text)
- [ ] Parse flags from common header bytes 1-2 (reader.rs)
- [ ] Write flags to common header (writer.rs, currently hardcoded to 0x00)

Flag bits to support:

- [ ] `Locked` (0x0001)
- [ ] `Polygon` (0x0002)
- [ ] `KeepOut` (0x0004)
- [ ] `TentingTop` (0x0008)
- [ ] `TentingBottom` (0x0010)

## OLE Streams - Partial Implementation

### Storage Stream

- [ ] Parse `UniqueIdPrimitiveInformation` mappings from `/Storage` stream
- [ ] Use mappings to link primitives to unique IDs

### FileHeader Improvements

- [ ] Parse `CompCount` field (number of components)
- [ ] Parse `LibRef{N}` fields (component names by index)
- [ ] Parse `CompDescr{N}` fields (component descriptions)
- [ ] Write complete FileHeader with all fields (currently minimal in mod.rs line 364-367)

## Code Quality

### Error Handling

- [ ] Return `Result` from `parse_pad()`, `parse_track()`, etc. instead of `Option`
- [ ] Add specific error types for parse failures
- [ ] Improve error messages with offset information

### Testing

- [x] Add roundtrip tests for Via primitive
- [x] Add tests for WideStrings parsing
- [x] Add tests for 3D model parsing and embedding
- [x] Add tests for pad hole shapes, mask expansion (basic features)
- [ ] Add tests for advanced pad features (stack modes, per-layer data)
- [ ] Add tests for all layer ID mappings
- [ ] Add integration tests with real Altium library files

### Documentation

- [ ] Add doc comments to all public types in primitives.rs
- [ ] Document coordinate system in primitives.rs module docs
- [ ] Add examples to Pad, Track, Arc struct docs

## Low Priority / Future

- [ ] Support reading/writing SchLib files (separate module exists but may need similar review)
- [ ] Support component variants (board-level feature, not library)
- [ ] Support net information (board-level feature, not library)
- [ ] Optimize binary parsing with zero-copy where possible
