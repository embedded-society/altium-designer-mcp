# TODO

## Future Enhancements

- [ ] Add integration tests with real Altium library files (requires sample .PcbLib/.SchLib files)
- [ ] Support component variants (board-level feature, not library)
- [ ] Support net information (board-level feature, not library)

---

## Altium Designer MCP Server - Testing Report

This document summarises the findings from extensive testing of the `altium-designer-mcp` server.

**Test Date:** 2026-01-24
**Tested Against:** Libraries in this repository

---

## Summary

| Category | Status |
|----------|--------|
| Read Operations | Working |
| Write Operations | Working |
| Style Extraction | Working |
| Error Handling | Good |
| Edge Cases | Mostly Handled |

---

## Detailed Findings

### 1. Read Operations

#### `read_pcblib`

- **Status:** Working
- Correctly reads all primitives: pads, tracks, arcs, regions, text
- Pagination (`limit`/`offset`) works correctly
- Component name filtering works (returns empty array for non-existent names)
- Returns detailed pad information including: shape, layer, rotation, hole_size, solder_mask_expansion
- Returns track flags (e.g., `KEEPOUT | TENTING_TOP`)

#### `read_schlib`

- **Status:** Working
- Correctly reads: pins, rectangles, lines, arcs, ellipses, polylines, labels
- Returns symbol parameters (Value, Part Number, Manufacturer, Comment)
- Returns linked footprints with descriptions
- Pin electrical types correctly mapped

#### `list_components`

- **Status:** Working
- Fast enumeration of component names
- Returns component count

#### `extract_style`

- **Status:** Working
- PcbLib: Returns layer usage, pad shapes, track widths by layer
- SchLib: Returns fill colours, line colours, pin lengths, rectangle statistics

---

### 2. Write Operations

#### `write_pcblib`

- **Status:** Working
- Successfully creates footprints with all primitive types
- Append mode works correctly
- **Supported pad shapes:** `rectangle`, `round`, `oval`, `rounded_rectangle`
- All pad shapes round-trip correctly

#### `write_schlib`

- **Status:** Working
- Successfully creates symbols with pins, rectangles, lines
- `designator_prefix` correctly converted to designator (e.g., "U" → "U?")
- All pin orientations work: `left`, `right`, `up`, `down`
- All electrical types work: `input`, `output`, `bidirectional`, `passive`, `power`
- Electrical type names are consistent (uses `bidirectional` throughout)

---

### 3. Validation & Error Handling

#### Good Validation

| Check | Behaviour |
|-------|----------|
| Non-existent file | Returns clear error message |
| Invalid characters in name (`/ \ : * ? " < > \|`) | Rejects with helpful error |
| Name too long (>31 bytes) | Rejects with explanation of OLE storage limit |
| Invalid layer name | Rejects with list of valid layers |
| Offset past end of list | Returns empty array gracefully |

#### Additional Validation (Implemented)

| Check | Behaviour |
|-------|----------|
| Zero-size pads | Rejected with clear error message |
| Empty pad designator | Rejected with clear error message |
| Duplicate component names | Rejected with clear error message |
| Non-existent component filter | Returns empty array (expected behaviour) |

---

### 4. Data Precision

Coordinates show minor floating-point artifacts when round-tripping:

- Input: `1.0` → Output: `1.000001`
- Input: `0.5` → Output: `0.499999`

This is likely due to internal unit conversion (mm ↔ mils) and is within acceptable tolerance for PCB design.

---

## Issues Found

### Critical

*None*

### Medium Priority

1. ~~**`rounded_rectangle` pad shape not preserved**~~ **FIXED**
   - Now correctly round-trips with default 50% corner radius

2. **Component name truncation in list_components**
   - Some names appear truncated (e.g., `GENERIC_MLCC_CAP_0402_IPC_MEDIU` instead of `MEDIUM`)
   - This is an Altium format limitation (31 byte OLE storage limit)

### Low Priority

1. **Floating-point precision artifacts**
   - Values like `1.0` become `1.000001`
   - Cosmetic issue, doesn't affect functionality

2. ~~**`bidirectional` ↔ `input_output` alias inconsistency**~~ **FIXED**
   - Now uses `bidirectional` consistently (accepts `input_output` as alias for backwards compatibility)

---

## Nice-to-Have Features

1. ~~**Parameter management for SchLib**~~ **IMPLEMENTED**
   - `manage_schlib_parameters` tool with list/get/set/add/delete operations

2. ~~**Footprint link management**~~ **IMPLEMENTED**
   - `manage_schlib_footprints` tool with list/add/remove operations

3. **Support for multi-part symbols**
   - Currently `part_count` is read but not writable

---

## Test Files Created

The following test files were created during testing and can be safely deleted:

- `test_output.PcbLib` - General write tests
- `test_output.SchLib` - SchLib write tests
- `test_edge_cases.PcbLib` - Edge case tests
- `test_edge_cases.SchLib` - SchLib edge cases

---

## Conclusion

The `altium-designer-mcp` server is **production-ready** for all common use cases. It provides reliable
read/write operations for Altium library files with comprehensive error handling and validation.

All pad shapes including `rounded_rectangle` now round-trip correctly. Pin electrical types use
consistent naming (`bidirectional`). Input validation catches common errors like zero-size pads,
empty designators, and duplicate component names.

The only remaining cosmetic issue is minor floating-point precision artifacts from mm↔mils conversion,
which doesn't impact functionality.

Recommended next steps:

1. Add support for multi-part symbols (writable `part_count`)
