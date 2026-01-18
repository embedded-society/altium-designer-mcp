# SchLib Binary Format

This document describes the binary format of Altium Designer `.SchLib` (Schematic symbol library) files.

> **Legend:** Items marked with `TODO` need implementation. Items marked with `UNKNOWN` are not fully understood.

## File Structure

SchLib files are OLE Compound Documents (CFB format) containing:

```text
/
├── FileHeader          # Library metadata
├── Storage             # Additional storage info (UNKNOWN: purpose unclear)
└── {ComponentName}/    # One storage per symbol
    └── Data            # Symbol primitives stream
```

## FileHeader Stream

The FileHeader contains library-level metadata as pipe-delimited key=value pairs:

```text
[length:4 LE][text...]
```

Key fields:

| Key | Description | Notes |
|-----|-------------|-------|
| `HEADER` | File type identifier | "Schematic Library Editor Binary File Version 5.0" |
| `CompCount` | Number of components | |
| `LibRef{N}` | Component name | 0-indexed |
| `CompDescr{N}` | Component description | |
| `PartCount{N}` | Number of parts | Stored as count+1 in file |
| `FontIdCount` | Number of custom fonts | |
| `FontName{N}` | Font name | |
| `Size{N}` | Font size | |

> **Note:** `PartCount` is stored as actual_count + 1 in the file. When reading, subtract 1 to get the true part count.

## Data Stream Format

Each component's Data stream contains the symbol primitives:

```text
[RecordLength:2 LE][RecordType:2 BE][data:RecordLength]
[RecordLength:2 LE][RecordType:2 BE][data:RecordLength]
...
[0x00 0x00]  # End marker (length = 0)
```

### Record Types (Header)

The 2-byte record type in the header determines how to parse the record data:

| Type | Format | Description |
|------|--------|-------------|
| `0x0000` | Text | Pipe-delimited key=value pairs (most primitives) |
| `0x0001` | Binary | Binary pin record (more compact than text) |

> **Note:** Most primitives use text format (type 0). Binary format (type 1) is used for pins to reduce file size.

## Text Records (Type 0)

Text records contain pipe-delimited key=value pairs:

```text
|RECORD=14|Location.X=-10|Location.Y=-4|Corner.X=10|Corner.Y=4|...
```

### Record IDs (RECORD= field)

| ID | Type | Status | Description |
|----|------|--------|-------------|
| 1 | Component | ✓ | Symbol header (name, description, part count) |
| 2 | Pin | TODO | Pin (text format, rarely used — binary preferred) |
| 4 | Label | TODO | Text label |
| 5 | Bezier | TODO | Bezier curve |
| 6 | Polyline | ✓ | Multiple connected line segments |
| 7 | Polygon | TODO | Filled polygon |
| 8 | Ellipse | ✓ | Ellipse or circle |
| 10 | RoundRectangle | TODO | Rounded rectangle |
| 11 | EllipticalArc | TODO | Elliptical arc |
| 12 | Arc | ✓ | Circular arc |
| 13 | Line | ✓ | Single line segment |
| 14 | Rectangle | ✓ | Rectangle shape |
| 34 | Designator | ✓ | Component designator (R?, U?, etc.) |
| 41 | Parameter | ✓ | Component parameter (Value, Part Number, etc.) |
| 44 | ImplementationList | TODO | Start of model/footprint list |
| 45 | Model | ✓ | Footprint model reference |
| 46 | ModelDatafileLink | TODO | Model data file reference |
| 47 | ModelDatafileEntity | TODO | Model data file entity |
| 48 | Implementation | TODO | Implementation details |

## Binary Pin Records (Type 1)

Binary pin records have a variable-length structure. Offsets after the description string are relative.

| Offset | Size | Field |
|--------|------|-------|
| 0-3 | 4 | Record type (always 2 for pin) |
| 4 | 1 | UNKNOWN |
| 5-6 | 2 | OwnerPartId (signed i16, LE) |
| 7 | 1 | OwnerPartDisplayMode |
| 8 | 1 | Symbol_InnerEdge (pin symbol decoration) |
| 9 | 1 | Symbol_OuterEdge (pin symbol decoration) |
| 10 | 1 | Symbol_Inside (pin symbol decoration) |
| 11 | 1 | Symbol_Outside (pin symbol decoration) |
| 12 | 1 | Description length |
| 13 | 1 | UNKNOWN |
| 14+ | N | Description string (ASCII) |

After description (relative offsets):

| Offset | Size | Field |
|--------|------|-------|
| +0 | 1 | Electrical_Type |
| +1 | 1 | Flags (see below) |
| +2-3 | 2 | Length (schematic units, i16) |
| +4-5 | 2 | Location.X (signed i16) |
| +6-7 | 2 | Location.Y (signed i16) |
| +8-11 | 4 | Color (BGR, UNKNOWN: may not always be present) |
| +12 | 1 | Name length |
| +13+ | N | Name string (ASCII) |

After name:

| Offset | Size | Field |
|--------|------|-------|
| +0 | 1 | Designator length |
| +1+ | N | Designator string (ASCII) |

> **UNKNOWN:** Symbol decoration bytes (8-11) control pin symbols like clock, dot, etc. Exact bit mappings need documentation.

### Pin Flags (byte at +1)

| Bit | Flag |
|-----|------|
| 0x01 | Rotated |
| 0x02 | Flipped |
| 0x04 | Hidden |
| 0x08 | Show Name |
| 0x10 | Show Designator |
| 0x20 | UNKNOWN |
| 0x40 | Graphically Locked |
| 0x80 | UNKNOWN |

### Pin Orientation

Derived from Rotated and Flipped flags:

| Rotated | Flipped | Orientation |
|---------|---------|-------------|
| false | false | Right (connection on left) |
| false | true | Left (connection on right) |
| true | false | Up (connection on bottom) |
| true | true | Down (connection on top) |

### Electrical Types

| ID | Type |
|----|------|
| 0 | Input |
| 1 | InputOutput (Bidirectional) |
| 2 | Output |
| 3 | OpenCollector |
| 4 | Passive |
| 5 | HiZ (Tri-state) |
| 6 | OpenEmitter |
| 7 | Power |

## Coordinate System

- Schematic units: **10 units = 1 grid square**
- Standard grid is 10 units
- Pins are typically 10-30 units long

## Colour Format

Colours are stored as 32-bit BGR values:

```text
0x00BBGGRR
```

Common colours:

| Value | Colour | Usage |
|-------|-------|-------|
| `0x000080` | Dark Red | Lines, outlines |
| `0x800000` | Dark Blue | Text, parameters |
| `0xB0FFFF` | Light Yellow | Fill colour |
| `0xFF0000` | Blue | Component body |
| `0x000000` | Black | Pins |

## Text Record Examples

### Component Header (RECORD=1)

```text
|RECORD=1|LibReference=SMD Chip Resistor|ComponentDescription=generic SMD chip resistor|PartCount=2|DisplayModeCount=1|...
```

### Rectangle (RECORD=14)

```text
|RECORD=14|Location.X=-10|Location.Y=-4|Corner.X=10|Corner.Y=4|LineWidth=1|Color=16711680|AreaColor=11599871|...
```

### Parameter (RECORD=41)

```text
|RECORD=41|Location.X=-22|Location.Y=-34|Color=8388608|FontID=1|IsHidden=T|Text=*|Name=Value|...
```

### Footprint Model (RECORD=45)

```text
|RECORD=45|OwnerIndex=0|Description=Generic Chip Resistor, 0805|ModelName=GENERIC_CHIP_RES_0805_IPC_MEDIUM_DENSITY|ModelType=PCBLIB|...
```

## Multi-Part Symbols

Some symbols have multiple parts (e.g., quad op-amp):

- `PartCount` in component header indicates total parts
- Each primitive has `OwnerPartId` field:
    - `-1` = belongs to all parts
    - `1+` = belongs to specific part

## Known Limitations

The following features are not fully understood or implemented:

| Feature | Status |
|---------|--------|
| Label (RECORD=4) | TODO: Text label primitive |
| Bezier (RECORD=5) | TODO: Bezier curve primitive |
| Polygon (RECORD=7) | TODO: Filled polygon primitive |
| RoundRectangle (RECORD=10) | TODO: Rounded rectangle |
| EllipticalArc (RECORD=11) | TODO: Elliptical arc |
| ImplementationList (RECORD=44) | TODO: Model list container |
| ModelDatafileLink (RECORD=46) | TODO: Simulation model reference |
| ModelDatafileEntity (RECORD=47) | TODO: Simulation model entity |
| Implementation (RECORD=48) | TODO: Implementation details |
| Pin text format (RECORD=2) | TODO: Rarely used, binary format preferred |
| Pin symbol decorations | UNKNOWN: Bits 8-11 of pin header |
| Display modes | UNKNOWN: How multiple display modes are stored |
| Font storage | UNKNOWN: Custom font embedding format |

## References

- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) - Python library for reading Altium files
- Sample analysis: `scripts/analyze_schlib.py`
