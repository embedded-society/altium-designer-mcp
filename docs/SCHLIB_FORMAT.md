# SchLib Binary Format

This document describes the binary format of Altium Designer `.SchLib` (Schematic symbol library) files.

> **Note:** This documentation is based on reverse engineering from AltiumSharp, pyAltiumLib, and sample file analysis.
> See [References](#references) for links.

## File Structure

SchLib files are OLE Compound Documents (CFB format) containing:

```text
/
├── FileHeader          # Library metadata
├── Storage             # Section key mappings (maps LibRef names to storage names)
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

| ID | Type | Description |
|----|------|-------------|
| 1 | Component | Symbol header (name, description, part count) |
| 2 | Pin | Pin (text format, rarely used — binary preferred) |
| 4 | Label | Text label |
| 5 | Bezier | Bezier curve |
| 6 | Polyline | Multiple connected line segments |
| 7 | Polygon | Filled polygon |
| 8 | Ellipse | Ellipse or circle |
| 10 | RoundRectangle | Rounded rectangle |
| 11 | EllipticalArc | Elliptical arc |
| 12 | Arc | Circular arc |
| 13 | Line | Single line segment |
| 14 | Rectangle | Rectangle shape |
| 34 | Designator | Component designator (R?, U?, etc.) |
| 41 | Parameter | Component parameter (Value, Part Number, etc.) |
| 44 | ImplementationList | Start of model/footprint list |
| 45 | Model | Footprint model reference |
| 46 | ModelDatafileLink | Model data file reference |
| 47 | ModelDatafileEntity | Model data file entity |
| 48 | Implementation | Implementation details |

## Binary Pin Records (Type 1)

Binary pin records have a variable-length structure. Offsets after the description string are relative.

| Offset | Size | Field |
|--------|------|-------|
| 0-3 | 4 | Record type (always 2 for pin) |
| 4 | 1 | Symbol line width |
| 5-6 | 2 | OwnerPartId (signed i16, LE) |
| 7 | 1 | OwnerPartDisplayMode |
| 8 | 1 | Symbol_InnerEdge (see Pin Symbols below) |
| 9 | 1 | Symbol_OuterEdge (see Pin Symbols below) |
| 10 | 1 | Symbol_Inside (see Pin Symbols below) |
| 11 | 1 | Symbol_Outside (see Pin Symbols below) |
| 12 | 1 | Description length |
| 13 | 1 | Formal type |
| 14+ | N | Description string (ASCII) |

After description (relative offsets):

| Offset | Size | Field |
|--------|------|-------|
| +0 | 1 | Electrical_Type |
| +1 | 1 | Flags (see below) |
| +2-3 | 2 | Length (schematic units, i16) |
| +4-5 | 2 | Location.X (signed i16) |
| +6-7 | 2 | Location.Y (signed i16) |
| +8-11 | 4 | Color (BGR format) |
| +12 | 1 | Name length |
| +13+ | N | Name string (ASCII) |

After name:

| Offset | Size | Field |
|--------|------|-------|
| +0 | 1 | Designator length |
| +1+ | N | Designator string (ASCII) |

### Pin Symbols

The four symbol positions (InnerEdge, OuterEdge, Inside, Outside) can each have one of these decorations:

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

### Pin Flags (byte at +1)

| Bit | Flag | Description |
|-----|------|-------------|
| 0x01 | Rotated | Pin rotated 90° |
| 0x02 | Flipped | Pin flipped |
| 0x04 | Hidden | Pin hidden from view |
| 0x08 | DisplayNameVisible | Show pin name |
| 0x10 | DesignatorVisible | Show pin designator |
| 0x20 | Reserved | Reserved |
| 0x40 | GraphicallyLocked | Pin is graphically locked |
| 0x80 | Reserved | Reserved |

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

## Primitive Details

### Rectangle (RECORD=14)

| Property | Type | Description |
|----------|------|-------------|
| `Location.X` | int | First corner X |
| `Location.Y` | int | First corner Y |
| `Corner.X` | int | Second corner X |
| `Corner.Y` | int | Second corner Y |
| `LineWidth` | int | Border line width |
| `Color` | int | Border colour (BGR) |
| `AreaColor` | int | Fill colour (BGR) |
| `IsSolid` | bool | Whether filled |
| `Transparent` | bool | Whether transparent |

### Polyline (RECORD=6)

| Property | Type | Description |
|----------|------|-------------|
| `LocationCount` | int | Number of vertices |
| `X{N}`, `Y{N}` | int | Vertex coordinates (1-indexed) |
| `LineWidth` | int | Line thickness |
| `Color` | int | Line colour (BGR) |
| `LineStyle` | int | 0=Solid, 1=Dashed, 2=Dotted |
| `StartLineShape` | int | Start endpoint shape |
| `EndLineShape` | int | End endpoint shape |
| `LineShapeSize` | int | Size of endpoint shapes |

**Line shapes:**

| ID | Shape |
|----|-------|
| 0 | None |
| 1 | Arrow |
| 2 | SolidArrow |
| 3 | Tail |
| 4 | SolidTail |
| 5 | Circle |
| 6 | Square |

### Label (RECORD=4)

| Property | Type | Description |
|----------|------|-------------|
| `Location.X` | int | Position X |
| `Location.Y` | int | Position Y |
| `Text` | string | Label text |
| `FontId` | int | Font reference |
| `Color` | int | Text colour (BGR) |
| `Orientation` | int | Text orientation |
| `Justification` | int | Text alignment |
| `IsMirrored` | bool | Mirror horizontally |
| `IsHidden` | bool | Hidden from view |

### Parameter (RECORD=41)

| Property | Type | Description |
|----------|------|-------------|
| `Location.X` | int | Position X |
| `Location.Y` | int | Position Y |
| `Name` | string | Parameter name |
| `Text` | string | Parameter value |
| `FontId` | int | Font reference |
| `Color` | int | Text colour (BGR) |
| `ShowName` | bool | Display name with value |
| `IsHidden` | bool | Hidden from view |
| `ReadOnlyState` | int | Read-only flag |
| `ParamType` | int | 0=String, 1=Boolean, 2=Integer, 3=Float |

### Implementation (RECORD=45)

| Property | Type | Description |
|----------|------|-------------|
| `Description` | string | Model description |
| `ModelName` | string | Model identifier |
| `ModelType` | string | Type (PCBLIB, SIM, etc.) |
| `IsCurrent` | bool | Active implementation |
| `DataFileCount` | int | Number of data files |
| `ModelDataFileKind{N}` | string | Data file references |

## Multi-Part Symbols

Some symbols have multiple parts (e.g., quad op-amp):

- `PartCount` in component header indicates total parts
- Each primitive has `OwnerPartId` field:
    - `-1` = belongs to all parts
    - `1+` = belongs to specific part

## Notes

- **ImplementationList (RECORD=44)**: Container for model list
- **ModelDatafileLink (RECORD=46)**: Simulation model reference
- **ModelDatafileEntity (RECORD=47)**: Simulation model entity
- **Implementation (RECORD=48)**: Additional implementation details
- **Pin text format (RECORD=2)**: Rarely used, binary format preferred
- **Pin symbol decorations**: Documented above (22 symbol types)
- **Display modes**: Stored in `DisplayModeCount`, primitives have `OwnerPartDisplayMode`
- **Font storage**: Fonts defined in FileHeader (`FontName{N}`, `Size{N}`)

## References

- [AltiumSharp](https://github.com/issus/AltiumSharp) - C# library for Altium files (MIT)
- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) - Python library for reading Altium files
- [python-altium](https://github.com/vadmium/python-altium) - Altium format documentation
- Sample analysis: `scripts/analyze_schlib.py`
