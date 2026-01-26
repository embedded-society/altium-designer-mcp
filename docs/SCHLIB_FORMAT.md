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
| `WEIGHT` | File weight/version | Typically 47 |
| `MINORVERSION` | Minor version number | Typically 9 |
| `UNIQUEID` | Library unique ID | 8-char alphanumeric |
| `CompCount` | Number of components | |
| `LibRef{N}` | Component name | 0-indexed |
| `CompDescr{N}` | Component description | |
| `PartCount{N}` | Number of parts | Stored as count+1 in file |
| `FontIdCount` | Number of custom fonts | Default: 1 |
| `FontName{N}` | Font name | Default: "Times New Roman" |
| `Size{N}` | Font size | Default: 10 |
| `UseMBCS` | Multibyte character set | "T" or "F" |
| `IsBOC` | Binary OLE Container flag | "T" or "F" |
| `SheetStyle` | Sheet style number | |
| `BorderOn` | Border enabled | "T" or "F" |
| `SnapGridOn` | Snap grid enabled | "T" or "F" |
| `SnapGridSize` | Snap grid size | Default: 10 |
| `VisibleGridOn` | Visible grid enabled | "T" or "F" |
| `VisibleGridSize` | Visible grid size | Default: 10 |
| `CustomX`, `CustomY` | Custom sheet dimensions | |
| `UseCustomSheet` | Use custom sheet | "T" or "F" |
| `AreaColor` | Background colour (BGR) | |

**Typical hardcoded values:**

| Key | Value |
|-----|-------|
| `WEIGHT` | 47 |
| `MINORVERSION` | 9 |
| `FontIdCount` | 1 |
| `Size1` | 10 |
| `FontName1` | Times New Roman |
| `UseMBCS` | T |
| `IsBOC` | T |
| `SheetStyle` | 9 |
| `BorderOn` | T |
| `SheetNumberSpaceSize` | 12 |
| `SnapGridSize` | 10 |
| `VisibleGridSize` | 10 |
| `CustomX`, `CustomY` | 18000 |
| `UseCustomSheet` | T |
| `ReferenceZonesOn` | T |
| `Display_Unit` | 0 |

> **Note:** `PartCount` is stored as actual_count + 1 in the file. When reading, subtract 1 to get the true part count.
> A minimum part count of 1 is enforced even if the file contains 0.

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

Binary pin records have a variable-length structure with three length-prefixed strings.

### Fixed Header (12 bytes)

| Offset | Size | Field | Notes |
|--------|------|-------|-------|
| 0-3 | 4 | Record type | Always 2 for pin (i32, LE) |
| 4 | 1 | Reserved | Unknown purpose |
| 5-6 | 2 | OwnerPartId | Signed i16, LE (-1 = all parts) |
| 7 | 1 | OwnerPartDisplayMode | Display mode (typically 0) |
| 8-11 | 4 | Symbol flags | 4 bytes: InnerEdge, OuterEdge, Inside, Outside |

> **Note:** Symbol flags are currently **not implemented** — the tool reads/writes zeros. See [Pin Symbols](#pin-symbols) for the full format specification.

### Description Block

| Offset | Size | Field |
|--------|------|-------|
| 12 | 1 | Description length (N) |
| 13 | 1 | Reserved |
| 14+ | N | Description string (ASCII) |

### Pin Properties (after description)

| Offset | Size | Field | Notes |
|--------|------|-------|-------|
| +0 | 1 | Electrical_Type | See [Electrical Types](#electrical-types) |
| +1 | 1 | Flags | See [Pin Flags](#pin-flags-byte-at-1) |
| +2-3 | 2 | Length | Schematic units, signed i16, LE |
| +4-5 | 2 | Location.X | Signed i16, LE |
| +6-7 | 2 | Location.Y | Signed i16, LE |
| +8-11 | 4 | Colour | BGR format (currently **not implemented** — defaults to black) |

### Name Block (after properties)

| Offset | Size | Field |
|--------|------|-------|
| +0 | 1 | Name length (N) |
| +1+ | N | Name string (ASCII) |

### Designator Block (after name)

| Offset | Size | Field |
|--------|------|-------|
| +0 | 1 | Designator length (N) |
| +1+ | N | Designator string (ASCII) |

### Pin Symbols

Pin symbol decorations appear at four positions on the pin to indicate electrical characteristics.

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

| Bit | Flag | Description | Implemented |
|-----|------|-------------|-------------|
| 0x01 | Rotated | Pin rotated 90° | ✓ |
| 0x02 | Flipped | Pin flipped | ✓ |
| 0x04 | Hidden | Pin hidden from view | ✓ |
| 0x08 | DisplayNameVisible | Show pin name | ✓ |
| 0x10 | DesignatorVisible | Show pin designator | ✓ |
| 0x20 | Reserved | Reserved | — |
| 0x40 | GraphicallyLocked | Pin is graphically locked | ✓ |
| 0x80 | Reserved | Reserved | — |

### Pin Constraints

| Field | Limit |
|-------|-------|
| Name | 255 bytes max |
| Designator | 255 bytes max |
| Description | 255 bytes max |
| Location.X, Location.Y | i16 range (±32767) |
| Length | i16 range (±32767) |

### Pin Defaults

| Field | Default |
|-------|---------|
| `electrical_type` | Passive (ID 4) |
| `show_name` | true |
| `show_designator` | true |
| `colour` | Black (0x000000) |

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
| 1 | Bidirectional (InputOutput) |
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

## Common Text Record Fields

Most text records include these standard fields:

| Property | Type | Description |
|----------|------|-------------|
| `RECORD` | int | Record type ID |
| `OwnerPartId` | int | Part ownership (-1 = all parts, 1+ = specific part) |
| `OwnerPartDisplayMode` | int | Display mode (typically 0) |
| `IndexInSheet` | int | Index within sheet (-1 for most) |
| `UniqueID` | string | 8-char alphanumeric identifier |
| `IsNotAccesible` | bool | Access flag ("T" or "F") |

> **Note:** `UniqueID` is present on all shape records for tracking across edits.

## Component Header (RECORD=1)

The component header contains symbol-level metadata:

| Property | Type | Description |
|----------|------|-------------|
| `LibReference` | string | Component name |
| `ComponentDescription` | string | Description |
| `PartCount` | int | Number of parts (stored as count+1) |
| `DisplayModeCount` | int | Number of display modes (typically 1) |
| `IndexInSheet` | int | Sheet index (-1) |
| `CurrentPartId` | int | Currently displayed part (1) |
| `SourceLibraryName` | string | Source library ("*") |
| `TargetFileName` | string | Target file ("*") |
| `AllPinCount` | int | Total number of pins (calculated from symbol) |
| `AreaColor` | int | Fill colour (BGR, default: 11599871 = light yellow) |
| `Color` | int | Border colour (BGR, default: 128 = dark red) |
| `PartIDLocked` | bool | Lock part ID ("T" or "F") |

**Typical hardcoded values:**

| Property | Value |
|----------|-------|
| `DisplayModeCount` | 1 |
| `IndexInSheet` | -1 |
| `CurrentPartId` | 1 |
| `SourceLibraryName` | * |
| `TargetFileName` | * |
| `AreaColor` | 11599871 (light yellow) |
| `Color` | 128 (dark red) |
| `PartIDLocked` | F |

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

> **Note:** Rectangle `IsSolid` is always written as `T` (filled). The `Transparent` property is not currently parsed.

### Polyline (RECORD=6)

| Property | Type | Description | Implemented |
|----------|------|-------------|-------------|
| `LocationCount` | int | Number of vertices | ✓ |
| `X{N}`, `Y{N}` | int | Vertex coordinates (1-indexed) | ✓ |
| `LineWidth` | int | Line thickness | ✓ |
| `Color` | int | Line colour (BGR) | ✓ |
| `LineStyle` | int | 0=Solid, 1=Dashed, 2=Dotted | ✓ |
| `StartLineShape` | int | Start endpoint shape | ✓ |
| `EndLineShape` | int | End endpoint shape | ✓ |
| `LineShapeSize` | int | Size of endpoint shapes | ✓ |
>
> Polylines require a minimum of 2 vertices.

### Polygon (RECORD=7)

| Property | Type | Description |
|----------|------|-------------|
| `LocationCount` | int | Number of vertices |
| `X{N}`, `Y{N}` | int | Vertex coordinates (1-indexed) |
| `LineWidth` | int | Border line width |
| `Color` | int | Border colour (BGR) |
| `AreaColor` | int | Fill colour (BGR) |
| `IsSolid` | bool | Whether border is solid |

> **Note:** When `IsSolid=T`, the polygon is filled. When `IsSolid=F`, only the outline is drawn. Default is filled.
>
> Polygons require a minimum of 3 vertices.

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
| `FontId` | int | Font reference (default: 1) |
| `Color` | int | Text colour (BGR) |
| `Orientation` | int | Text orientation (0-3) |
| `Justification` | int | Text alignment (see below) |
| `IsMirrored` | bool | Mirror horizontally |
| `IsHidden` | bool | Hidden from view |

**Orientation values:**

| Value | Rotation |
|-------|----------|
| 0 | 0° (horizontal) |
| 1 | 90° |
| 2 | 180° |
| 3 | 270° |

Formula: `orientation = (rotation_degrees / 90) % 4`

**Justification values:**

| ID | Position |
|----|----------|
| 0 | Bottom Left |
| 1 | Bottom Centre |
| 2 | Bottom Right |
| 3 | Middle Left |
| 4 | Middle Centre |
| 5 | Middle Right |
| 6 | Top Left |
| 7 | Top Centre |
| 8 | Top Right |

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

### Designator (RECORD=34)

The designator record identifies the component (e.g., R?, U?, C?).

| Property | Type | Description |
|----------|------|-------------|
| `Location.Y` | int | Position Y (typically -6) |
| `Text` | string | Designator text |
| `FontId` | int | Font reference |
| `Color` | int | Text colour (BGR) |
| `Name` | string | Always "Designator" |
| `ReadOnlyState` | int | Read-only flag |

**Typical hardcoded values:**

| Property | Value |
|----------|-------|
| `Location.Y` | -6 |
| `Color` | 8388608 (dark blue) |
| `FontID` | 1 |
| `Name` | Designator |
| `ReadOnlyState` | 1 |
| `IndexInSheet` | -1 |
| `OwnerPartId` | -1 |

### Implementation (RECORD=45)

| Property | Type | Description |
|----------|------|-------------|
| `Description` | string | Model description |
| `ModelName` | string | Model identifier |
| `ModelType` | string | Type (PCBLIB, SIM, etc.) |
| `IsCurrent` | bool | Active implementation |
| `DataFileCount` | int | Number of data files |
| `ModelDataFileKind{N}` | string | Data file references |

### EllipticalArc (RECORD=11)

| Property | Type | Description |
|----------|------|-------------|
| `Location.X` | int | Centre X |
| `Location.Y` | int | Centre Y |
| `Radius` | int | Primary radius (integer part) |
| `Radius_Frac` | int | Primary radius fractional part (× 100,000) |
| `SecondaryRadius` | int | Secondary radius (integer part) |
| `SecondaryRadius_Frac` | int | Secondary radius fractional part (× 100,000) |
| `StartAngle` | float | Start angle (degrees) |
| `EndAngle` | float | End angle (degrees) |
| `LineWidth` | int | Line width |
| `Color` | int | Line colour (BGR) |

> **Note:** Fractional radius values are stored multiplied by 100,000 for precision without floating point.
> Maximum fractional value is 99999 (clamped during writing).
>
> `Location.Y` may be omitted if the value is 0.
>
> Default values: `StartAngle=0.0`, `EndAngle=360.0` (full ellipse).

### Arc (RECORD=12)

| Property | Type | Description |
|----------|------|-------------|
| `Location.X` | int | Centre X |
| `Location.Y` | int | Centre Y |
| `Radius` | int | Arc radius |
| `StartAngle` | float | Start angle (degrees, default: 0.0) |
| `EndAngle` | float | End angle (degrees, default: 360.0) |
| `LineWidth` | int | Line width |
| `Color` | int | Line colour (BGR) |

> **Note:** Default angles create a full circle when not specified.

### Line (RECORD=13)

| Property | Type | Description |
|----------|------|-------------|
| `Location.X` | int | Start X |
| `Location.Y` | int | Start Y |
| `Corner.X` | int | End X |
| `Corner.Y` | int | End Y |
| `LineWidth` | int | Line width |
| `Color` | int | Line colour (BGR) |

### Ellipse (RECORD=8)

| Property | Type | Description |
|----------|------|-------------|
| `Location.X` | int | Centre X |
| `Location.Y` | int | Centre Y |
| `Radius` | int | X radius |
| `SecondaryRadius` | int | Y radius |
| `LineWidth` | int | Border line width |
| `Color` | int | Border colour (BGR) |
| `AreaColor` | int | Fill colour (BGR) |
| `IsSolid` | bool | Whether filled |

> **Note:** If `SecondaryRadius` is missing, it defaults to the value of `Radius` (circle).

### RoundRectangle (RECORD=10)

| Property | Type | Description |
|----------|------|-------------|
| `Location.X` | int | First corner X |
| `Location.Y` | int | First corner Y |
| `Corner.X` | int | Second corner X |
| `Corner.Y` | int | Second corner Y |
| `CornerXRadius` | int | Corner X radius |
| `CornerYRadius` | int | Corner Y radius |
| `LineWidth` | int | Border line width |
| `Color` | int | Border colour (BGR) |
| `AreaColor` | int | Fill colour (BGR) |
| `IsSolid` | bool | Whether filled |

### Bezier (RECORD=5)

| Property | Type | Description |
|----------|------|-------------|
| `LocationCount` | int | Number of control points (always 4) |
| `X{N}`, `Y{N}` | int | Control point coordinates (1-indexed) |
| `LineWidth` | int | Line width |
| `Color` | int | Line colour (BGR) |

> **Note:** Bezier curves always have exactly 4 control points. The `LocationCount` value is not validated during parsing.

## Default Values

Common default values used when properties are not specified:

| Property | Default | Notes |
|----------|---------|-------|
| `FontId` | 1 | Times New Roman, 10pt |
| `StartAngle` | 0.0 | For Arc and EllipticalArc |
| `EndAngle` | 360.0 | For Arc and EllipticalArc |
| `OwnerPartId` | 1 | First part (shapes default to 1, not -1) |
| `OwnerPartDisplayMode` | 0 | Default display mode |
| `IndexInSheet` | -1 | No specific index |
| `LineWidth` | 1 | All shapes |
| `Color` (lines) | 0x000080 | Dark red (BGR) |
| `Color` (text) | 0x800000 | Dark blue (BGR) |
| `AreaColor` | 0xFFFFB0 | Light yellow (BGR) |
| `PartCount` | 1 | Minimum enforced |

## Symbol Writing Order

When writing symbol data, records are encoded in this specific order:

1. Component header (RECORD=1)
2. Parameters (RECORD=41)
3. Pins (binary format, type 0x0001)
4. Rectangles (RECORD=14)
5. Lines (RECORD=13)
6. Polylines (RECORD=6)
7. Polygons (RECORD=7)
8. Arcs (RECORD=12)
9. Bezier curves (RECORD=5)
10. Ellipses (RECORD=8)
11. Rounded rectangles (RECORD=10)
12. Elliptical arcs (RECORD=11)
13. Labels (RECORD=4)
14. Designator (RECORD=34)
15. Implementation list (RECORD=44)
16. Footprint models (RECORD=45)
17. End marker (0x0000)

> **Note:** The `IndexInSheet` counter is incremented for each shape record but NOT for pins.

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
- **Pin symbol decorations**: Supported (22 symbol types)
- **Pin colour**: Stored in binary format (BGR)
- **Display modes**: Stored in `DisplayModeCount`, primitives have `OwnerPartDisplayMode`
- **Font storage**: Fonts defined in FileHeader (`FontName{N}`, `Size{N}`)
- **Unique IDs**: All shapes have 8-char alphanumeric `UniqueID` for tracking
- **Polyline styles**: `LineStyle`, `StartLineShape`, `EndLineShape`, `LineShapeSize` supported

## References

- [AltiumSharp](https://github.com/issus/AltiumSharp) - C# library for Altium files (MIT)
- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) - Python library for reading Altium files
- [python-altium](https://github.com/vadmium/python-altium) - Altium format documentation
- Sample analysis: `scripts/analyze_schlib.py`
