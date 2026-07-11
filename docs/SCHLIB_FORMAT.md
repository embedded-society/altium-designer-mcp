# SchLib Binary Format

This document describes the binary format of Altium Designer `.SchLib` (Schematic symbol library)
files as implemented by this crate. Every field below is verified against the byte-level
reverse-engineering campaign, the regenerated golden fixtures (`scripts/samples/symbols.SchLib`,
authored by real Altium 24) and the current reader/writer in `src/altium/schlib/`.

> **Note:** Cross-referenced against AltiumSharp, pyAltiumLib and Altium-authored sample libraries.
> See [References](#references) for links.

## File Structure

SchLib files are OLE Compound Documents (CFB format, OLE v3) containing:

```text
/
├── FileHeader              # Library metadata (C-string param block)
├── Storage                 # Embedded image bytes (compressed-storage stream)
└── {ComponentName}/        # One storage per symbol
    ├── Data                # Symbol records stream
    ├── PinFrac             # OPTIONAL: fractional pin coordinates (compressed storage)
    └── PinSymbolLineWidth  # OPTIONAL: per-pin symbol line widths (compressed storage)
```

The two pin auxiliary streams are emitted only when at least one pin needs them (see
[Pin auxiliary streams](#pin-auxiliary-streams)); a symbol with on-grid, default-width pins has
only its `Data` stream, byte-identical to Altium's own output.

## Cross-Cutting Conventions

These rules apply to every text record and are referenced throughout the per-record tables
instead of being repeated:

- **Omit-when-default:** zero-valued numeric keys and false booleans are OMITTED from the record
  (Altium's `AddNonZero` / `AddBool` helpers). A missing key means its default — numeric `0`,
  boolean false. Boolean keys are written as `=T` only; `=F` is never written.
- **`IsNotAccesible`:** Altium's own single-'s' misspelling of "IsNotAccessible". Emitted as
  `IsNotAccesible=T` when set, omitted when false.
- **Colours are BGR:** 32-bit `0x00BBGGRR` (e.g. `128` = dark red, `8388608` = dark blue,
  `11599871` = light yellow, `16711680` = blue). An absent colour key reads back as 0 (black).
- **DXP units:** coordinates are integer DXP units where **10 units = 1 grid square** (1 DXP unit
  = 10 mil = 100,000 raw internal units). Sub-unit precision uses the `_Frac` companion keys (see
  [Fractional coordinates](#fractional-coordinates)).
- **Signed `_Frac`:** the fractional companion is signed, truncation is toward zero, and a zero
  integer part is omitted when the fraction is non-zero — golden-verified 2026-07-11 (details
  below).
- **`IndexInSheet`:** one shared sequential 0-based counter over all content records (see
  [IndexInSheet](#indexinsheet)).
- **`%UTF8%` keys:** a text value that Windows-1252 cannot represent (Cyrillic, CJK, Greek `Ω`,
  ...) is stored under a `%UTF8%<Key>` key (e.g. `%UTF8%Text`) holding the raw UTF-8 bytes,
  INSTEAD of the lossy plain `<Key>` — only one of the two is ever written. Applies to `Text` on
  Label, Text, Parameter, Designator and TextFrame records.
- **`UniqueID`:** 8-character alphanumeric per-record id, emitted as the LAST key.
- **Encoding:** records are Windows-1252, with a leading `|`, no trailing `|`, and a trailing
  `0x00` (the record length includes the null).

## FileHeader Stream

A single C-string parameter block:

```text
[block_len:4 LE]["|HEADER=...|Weight=47|..." + 0x00]    # length INCLUDES the null terminator
```

Keys as written by this crate (matching the golden library; note the mixed-case key spellings —
the reader is case-insensitive):

| Key | Value | Notes |
|-----|-------|-------|
| `HEADER` | `Protel for Windows - Schematic Library Editor Binary File Version 5.0` | File type identifier |
| `Weight` | 47 | File weight |
| `MinorVersion` | 9 | Minor version number |
| `UniqueID` | 8-char alphanumeric | Library unique ID |
| `FontIdCount` | 1 | Number of fonts in the font table |
| `Size1` | 10 | Font 1 size |
| `FontName1` | Times New Roman | Font 1 name |
| `UseMBCS` | T | Multibyte character set |
| `IsBOC` | T | Binary OLE container flag |
| `SheetStyle` | 9 | Sheet style number |
| `BorderOn` | T | Border enabled |
| `SheetNumberSpaceSize` | 12 | |
| `AreaColor` | 16317695 | Background colour (BGR) |
| `SnapGridOn` / `SnapGridSize` | T / 10 | |
| `VisibleGridOn` / `VisibleGridSize` | T / 10 | |
| `CustomX`, `CustomY` | 18000 | Custom sheet dimensions |
| `UseCustomSheet` | T | |
| `ReferenceZonesOn` | T | |
| `Display_Unit` | 0 | |
| `CompCount` | N | Number of components |
| `LibRef{i}` | name | Component name (0-indexed; OLE-safe storage name) |
| `CompDescr{i}` | text | Component description |
| `PartCount{i}` | N+1 | Stored as **count + 1** |

> **Note:** every `PartCount` in the format (here and in RECORD=1) is stored as
> `actual_count + 1`. Read as `max(0, stored - 1)` — do NOT floor at 1: a single-part symbol
> stores `PartCount=1`, which must decode to internal 0 and re-emit as 1 (flooring corrupted the
> round-trip to `PartCount=2`).

## Data Stream Format

Each component's Data stream contains the symbol records:

```text
[length:3 LE][flags:1][data:length]
[length:3 LE][flags:1][data:length]
...
```

The 4-byte header is a single 32-bit little-endian size word: the low 24 bits are the payload
length and the high byte is a flag (`0x00` = text record, `0x01` = binary pin). For payloads under
16 MiB (always, in practice) this is byte-identical to a `[u16 length LE][u16 BE type]` reading,
which is why earlier notes described it that way. For text records the length INCLUDES the
trailing `0x00`.

There is **no end-of-stream marker** — records simply run until the stream is exhausted. A trailing
`0x0000` would be mis-read as a zero-length record (this was part of issue #68; the writer must not
emit one).

### Record Types (Header flag byte)

| Flag | Format | Description |
|------|--------|-------------|
| `0x00` | Text | Pipe-delimited key=value pairs (most records) |
| `0x01` | Binary | Binary pin record (more compact than text) |

## Text Records (Type 0)

Text records contain pipe-delimited key=value pairs:

```text
|RECORD=14|Location.X=-10|Location.Y=-4|Corner.X=10|Corner.Y=4|...
```

### Record IDs (RECORD= field)

Every record type this crate models:

| ID | Type | Description |
|----|------|-------------|
| 1 | Component | Symbol header (name, description, part count) |
| 2 | Pin | Pin in text form (rare — skipped on read; binary pins are used instead) |
| 3 | Text | Text annotation (general-purpose text) |
| 4 | Label | Text label |
| 5 | Bezier | Cubic Bezier curve (4 control points) |
| 6 | Polyline | Multiple connected line segments |
| 7 | Polygon | Filled polygon |
| 8 | Ellipse | Ellipse or circle |
| 9 | Pie | Filled circular sector |
| 10 | RoundRectangle | Rounded rectangle |
| 11 | EllipticalArc | Elliptical arc |
| 12 | Arc | Circular arc |
| 13 | Line | Single line segment |
| 14 | Rectangle | Rectangle shape |
| 28 | TextFrame | Bordered multi-line text box |
| 30 | Image | Embedded/linked picture (bytes in `/Storage`) |
| 34 | Designator | Component designator (R?, U?, etc.) — a parameter record variant |
| 41 | Parameter | Component parameter (Value, Part Number, etc.) |
| 44 | ImplementationList | Container for a component's implementations |
| 45 | Implementation | One implementation (footprint model link) |
| 46 | MapDefinerList | Container for the implementation's map definers |
| 47 | MapDefiner | Pin-to-pad mapping (structure known; skipped on read) |
| 48 | ImplementationParameters | Per-implementation parameters |

Records 44/46/47/48 carry structural links only; this crate writes 44, 46 and 48 (46/48 as empty
`OwnerIndex`-bearing children of each RECORD=45) and skips all four on read. Record IDs that only
occur in schematic *documents* (Wire, Port, PowerObject, Note, ...) never occur in a `.SchLib` and
are out of scope.

## Binary Pin Records (Type 1)

Binary pin records have a variable-length structure: a fixed header, then Pascal short strings
(`[u8 len][bytes]`, Windows-1252) interleaved with fixed fields. Field order is strict. With `N` =
description length, the layout is:

| Offset | Size | Field | Notes |
|--------|------|-------|-------|
| 0-3 | 4 | Record type | Always 2 for pin (i32, LE) |
| 4 | 1 | Reserved | Always `0x00` |
| 5-6 | 2 | OwnerPartId | Signed i16, LE (-1 = all parts) |
| 7 | 1 | OwnerPartDisplayMode | Alternate-view index (0 in practice) |
| 8 | 1 | Symbol: InnerEdge | See [Pin Symbols](#pin-symbols) |
| 9 | 1 | Symbol: OuterEdge | |
| 10 | 1 | Symbol: Inside | |
| 11 | 1 | Symbol: Outside | |
| 12 | 1+N | Description | Pascal short string — NO interior reserved byte |
| 13+N | 1 | **FormalType** | `0x01` for a normal pin; AFTER Description, BEFORE Electrical |
| 14+N | 1 | Electrical type | See [Electrical Types](#electrical-types) |
| 15+N | 1 | Flags | See [Pin Flags](#pin-flags) |
| 16+N | 2 | Length | DXP units, signed i16 LE (integer part; fraction in `PinFrac`) |
| 18+N | 2 | Location.X | Signed i16, LE (integer part) |
| 20+N | 2 | Location.Y | Signed i16, LE (integer part) |
| 22+N | 4 | Colour | BGR, u32 LE |
| 26+N | 1+M | Name | Pascal short string |
| after | 1+K | Designator | Pascal short string |
| after | 1+P | SwapIdGroup | Pascal short string (empty by default) |
| after | 1+Q | PartAndSequence | Pascal short string; default `\|&\|` (= `{SwapIdPart}\|&\|{SwapIdSequence}` with both empty) |
| after | 1+R | DefaultValue | Pascal short string (e.g. `3.3V`); LAST field |

A truncated legacy record is read tolerantly: any absent trailing Pascal string reads as `""` and
is reproduced exactly on write (an empty `PartAndSequence` is NOT coerced back to `|&|`).

### Pin Flags

Flags byte at `15+N`:

| Bit | Flag | Description |
|-----|------|-------------|
| 0x01 | Rotated | Pin rotated 90° |
| 0x02 | Flipped | Pin flipped |
| 0x04 | Hidden | Pin hidden from view |
| 0x08 | DisplayNameVisible | Show pin name |
| 0x10 | DesignatorVisible | Show pin designator |
| 0x20 | IsNotAccessible | Not selectable |
| 0x40 | GraphicallyLocked | Pin is graphically locked |
| 0x80 | Reserved | — |

### Pin Symbols

Pin symbol decorations appear at four positions on the pin (bytes 8-11) to indicate electrical
characteristics:

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

### Pin Constraints

| Field | Limit |
|-------|-------|
| Name / Designator / Description / SwapIdGroup / PartAndSequence / DefaultValue | 255 **encoded** (Windows-1252) bytes max |
| Location.X, Location.Y | i16 range (±32767) |
| Length | i16 range (±32767) |

### Pin Defaults

| Field | Default |
|-------|---------|
| `electrical_type` | Passive (ID 4) |
| `formal_type` | 1 |
| `show_name` / `show_designator` | true |
| `colour` | Black (0x000000) |
| `part_and_sequence` | `\|&\|` |
| `swap_id_group` / `default_value` | empty |

### Pin Orientation

Derived from the Rotated and Flipped flags:

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

### Pin auxiliary streams

Two optional per-component OLE streams carry data the binary pin record cannot hold. Both use the
[compressed-storage framing](#compressed-storage-framing) with each entry keyed by the **pin
ordinal** as an ASCII-decimal Pascal string:

- **`PinFrac`** — the fractional part of each off-grid pin's X / Y / length. Payload: 12 bytes =
  three little-endian `i32` (`frac_x`, `frac_y`, `frac_length`), each scaled by 100,000 like the
  text-record `_Frac` keys.
- **`PinSymbolLineWidth`** — a per-pin symbol line width. Payload: a Unicode parameter block
  `[u32 LE byte_len][UTF-16LE "|SYMBOL_LINEWIDTH=N"]`.

A symbol whose pins are all on-grid with default line width emits **neither** stream.

## Compressed-Storage Framing

Three stream families share one byte layout: `PinFrac`, `PinSymbolLineWidth` and the root
`/Storage` stream:

```text
[u32 LE header_len][header_len header bytes]         # C-string param block
then, per entry:
  [u32 LE size]        # low 24 bits = block size, high byte = 0x01 flag
  0xD0                 # storage-entry tag
  [u8 name_len][name]  # Pascal-string entry key (Windows-1252)
  [u32 LE comp_len][comp_len bytes]   # zlib-compressed payload
```

The header param block is `|HEADER=<name>` plus `|Weight=<count>` (Altium's mixed-case key) when
at least one entry follows.

### `/Storage` (embedded images)

The root `/Storage` stream carries the raw bytes of every embedded image (`RECORD=30` with
`EmbedImage=T`), one compressed entry per image:

- **Header:** `|HEADER=Icon storage|Weight=<count>`. An **empty** library carries the bare
  `|HEADER=Icon storage` block with NO `Weight` key.
- **Entry names:** real AD24 names each entry with the image's **full file path** (the record's
  `FileName` value); AltiumSharp's own writer uses the zero-based index instead. This crate
  follows real AD24 on write.
- **Matching:** entry names are ignored on read — payloads are matched to `EmbedImage=T` images
  **in order across all symbols** (global stream order), exactly like AltiumSharp's
  `ParseStorageImageData`.
- **Payload:** the raw image file bytes (BMP/PNG/JPG), zlib-compressed (RFC 1950).

## Coordinate System

- Schematic units: **10 units = 1 grid square**
- Standard grid is 10 units
- Pins are typically 10-30 units long

### Fractional coordinates

Graphic-primitive coordinates may sit off the integer grid. Altium stores each
coordinate as an integer key plus an optional `<key>_Frac` companion holding the
fractional part scaled by 100,000, reconstructed as:

```text
value = <key> + <key>_Frac / 100000
```

This applies to every coordinate key — `Location.X`/`Y`, `Corner.X`/`Y`,
`Radius`, `SecondaryRadius`, `CornerXRadius`/`CornerYRadius`, `TextMargin` and
the polyline / polygon vertices `X{n}`/`Y{n}`. The `_Frac` field is **signed**:
AD24 truncates the integer part **toward zero** and lets the fraction carry the
coordinate's sign (range `-99999..=99999`, never opposite in sign to the integer
part). The FRACSHAPES golden fixture stores `-5.45` as `Location.X=-5` with
`Location.X_Frac=-45000` (`-5 + -45000/100000 = -5.45`). AD24 **omits every
zero coordinate key**, integer and `_Frac` alike: the golden arc centred at
`0.05` carries only `Location.X_Frac=5000` (no `Location.X`), the LINES golden
line `(0,0)→(10,0)` carries only `Corner.X=10`, and the POLYLINES golden
vertices omit their zero `X{n}`/`Y{n}` halves. The reader defaults every
absent coordinate key to `0`; integer-grid coordinates carry no `_Frac`. When the
fractional part rounds up to a whole unit it **carries** into the integer part
(e.g. `4.999995` → `Radius=5`, no `_Frac`) rather than being clamped.

> **Historical note:** versions of this crate before the signed-frac fix wrote
> the *floor* form instead — a non-negative `_Frac` with a floored integer part
> (`-5.45` → `Location.X=-6`, `Location.X_Frac=55000`). Both forms decode
> identically under `value = int + frac / 100000`, and the reader parses the
> fraction as a signed integer, so files written in either convention read
> back correctly. (The pre-fix reader parsed `_Frac` as *unsigned*, silently
> dropping the fractional part of every AD24-written negative off-grid
> coordinate.)

Binary pin records (Type 1) store integer coordinates only; their fractional
parts live in the separate `PinFrac` stream.

## Colour Format

Colours are stored as 32-bit BGR values:

```text
0x00BBGGRR
```

Common colours:

| Value | Colour | Usage |
|-------|-------|-------|
| `0x000080` (128) | Dark Red | Component outline |
| `0x800000` (8388608) | Dark Blue | Text, parameters |
| `0xB0FFFF` (11599871) | Light Yellow | Fill colour |
| `0xFF0000` (16711680) | Blue | Component body |
| `0x000000` (0) | Black | Pins; the read-back default for any absent colour key |

## IndexInSheet

All content records — every graphic shape, user Label/Parameter record **and** every binary pin —
share ONE sequential 0-based `IndexInSheet` counter in stream order (golden-confirmed against both
the regenerated fixture and real Altium-authored libraries):

- The token is **omitted at slot 0** and sits immediately after `IsNotAccesible` (before
  `OwnerPartId`), matching the golden token order `|RECORD=12|IsNotAccesible=T|IndexInSheet=1|…`.
- **Binary pins store no token** (the binary record has no such field) but still consume a
  counter slot: a real Altium symbol with parameters 0-2, two pins, then a rectangle stores
  `IndexInSheet=5` on the rectangle (slots 3 and 4 are the pins).
- The component header (RECORD=1) and the trailing system Designator (RECORD=34) / Comment
  (RECORD=41, `OwnerPartId=-1`) records carry the `IndexInSheet=-1` sentinel and do **not**
  consume a counter slot (the golden DISPMODE system Comment stores `IndexInSheet=-1` while the
  rectangles keep slots 0 and 1); RECORD=44/46/48 carry no token and RECORD=45 carries `-1`.
- The value is purely positional, so this crate derives it on write rather than storing it.

## Common Text Record Fields

Most text records include these standard fields (see
[Cross-Cutting Conventions](#cross-cutting-conventions) for the omission rules):

| Property | Type | Description |
|----------|------|-------------|
| `RECORD` | int | Record type ID (first key) |
| `IsNotAccesible` | bool | Not selectable; single-'s' spelling; emit only when `T` |
| `IndexInSheet` | int | Shared content counter (0 omitted; -1 on header/system records) |
| `OwnerPartId` | int | Part ownership (-1 = all parts, 1+ = specific part) |
| `OwnerPartDisplayMode` | int | Alternate display mode; omitted when 0 |
| `GraphicallyLocked` / `Disabled` / `Dimmed` | bool | Universal display/lock flags; emit only when `T` |
| `UniqueID` | string | 8-char alphanumeric identifier, last key |

The four universal display/lock flags (`GraphicallyLocked`, `Disabled`, `Dimmed`,
`OwnerPartDisplayMode`) are modelled on **all** shape records and sit **immediately after
`OwnerPartId`** (golden: `…|OwnerPartId=1|OwnerPartDisplayMode=1|Location.X=…` on the DISPMODE
rectangle, `…|OwnerPartId=1|GraphicallyLocked=T|…` on LOCKFLAGS), in the order
`OwnerPartDisplayMode`, `GraphicallyLocked`, `Disabled`, `Dimmed`.

## Component Header (RECORD=1)

The first record of each component's Data stream. Keys as written (in order):

| Property | Type | Description |
|----------|------|-------------|
| `LibReference` | string | Component name |
| `ComponentDescription` | string | Description |
| `PartCount` | int | **Stored as count + 1** (see FileHeader note) |
| `DisplayModeCount` | int | Number of display modes (typically 1) |
| `IndexInSheet` | int | -1 for the component root |
| `OwnerPartId` | int | -1 for the component root |
| `CurrentPartId` | int | Currently displayed part (default 1) |
| `LibraryPath` | string | `*` sentinel |
| `SourceLibraryName` | string | `*` sentinel |
| `SheetPartFileName` | string | `*` sentinel |
| `TargetFileName` | string | `*` sentinel |
| `AllPinCount` | int | Total number of pins (calculated from the symbol) |
| `AreaColor` | int | Fill colour (BGR, 11599871 = light yellow) |
| `Color` | int | Border colour (BGR, 128 = dark red) |
| `PartIDLocked` | bool | `T`/`F` |

> **Note:** Altium-authored headers also carry a component `UniqueID` and may carry
> `DesignItemId` / `ComponentKind`; these are currently unmodelled (dropped on read).

## Primitive Records

All shape records carry the [common fields](#common-text-record-fields) in addition to the tables
below; every coordinate accepts a `_Frac` companion.

### Text (RECORD=3) and Label (RECORD=4)

Two distinct record types sharing one field set. This crate reads/writes RECORD=3 as a
general-purpose text annotation and RECORD=4 as a label.

| Property | Type | Description |
|----------|------|-------------|
| `Location.X` / `Location.Y` | coord | Anchor position |
| `Orientation` | int | 0-3 = 0°/90°/180°/270°; omit at 0 |
| `Justification` | int | Text alignment (see below); omit at 0 |
| `Color` | int | Text colour (BGR; omit at 0) |
| `FontId` | int | Font-table reference (default 1; always written) |
| `Text` / `%UTF8%Text` | string | Content |
| `IsHidden` | bool | Emit only when `T` |
| `IsMirrored` | bool | Emit only when `T` |

Keys in golden order: `Orientation` and `Justification` sit between the coordinates and
`Color`/`FontID` (the JUSTIFY golden stores `…|Location.X=-10|Justification=8|FontID=1|Text=TR|…`).

**Orientation values:** `orientation = (rotation_degrees / 90) % 4`.

**Justification values:**

| ID | Position | ID | Position | ID | Position |
|----|----------|----|----------|----|----------|
| 0 | Bottom Left | 3 | Middle Left | 6 | Top Left |
| 1 | Bottom Centre | 4 | Middle Centre | 7 | Top Centre |
| 2 | Bottom Right | 5 | Middle Right | 8 | Top Right |

### Bezier (RECORD=5)

| Property | Type | Description |
|----------|------|-------------|
| `LocationCount` | int | Control point count (always 4; not validated on read) |
| `X{n}` / `Y{n}` | coord | Control points, 1-indexed (`X1`..`Y4`) |
| `LineWidth` | int | Line width index |
| `Color` | int | Line colour (BGR; omit at 0) |
| `IsNotAccesible` | bool | Emit only when `T` |

### Polyline (RECORD=6)

| Property | Type | Description |
|----------|------|-------------|
| `IsNotAccesible` | bool | Emit only when `T` (the golden tags every polyline) |
| `LineWidth` | int | Line width index (always written) |
| `LineStyle` | int | 0=Solid, 1=Dashed, 2=Dotted; omit at 0 |
| `StartLineShape` / `EndLineShape` | int | Endpoint shapes (see below); omit at 0 |
| `LineShapeSize` | int | Size of endpoint shapes; omit at 0 |
| `Color` | int | Line colour (BGR; omit at 0) |
| `Transparent` | bool | Emit only when `T`, before `LocationCount` |
| `LocationCount` | int | Vertex count (minimum 2) |
| `X{n}` / `Y{n}` | coord | Vertices, 1-indexed (zero halves omitted) |
| `LineStyleExt` | int | Style companion after the vertices, same value as `LineStyle`; omit at 0 |

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

### Polygon (RECORD=7)

| Property | Type | Description |
|----------|------|-------------|
| `IsNotAccesible` | bool | Emit only when `T` |
| `LineWidth` | int | Border width index |
| `Color` | int | Border colour (BGR; omit at 0) |
| `AreaColor` | int | Fill colour (BGR; omit at 0) |
| `LineStyle` | int | 0=Solid, 1=Dashed, 2=Dotted; omit at 0 |
| `IsSolid` | bool | Whether **filled**; emit only when `T` (absent = unfilled) |
| `Transparent` | bool | Emit only when `T`, before `LocationCount` (SHAPESTYLE golden) |
| `LocationCount` | int | Vertex count (minimum 3) |
| `X{n}` / `Y{n}` | coord | Vertices, 1-indexed (zero halves omitted) |

> **Note:** `IsSolid` is the **fill** flag, not a border style.

### Ellipse (RECORD=8)

| Property | Type | Description |
|----------|------|-------------|
| `IsNotAccesible` | bool | Emit only when `T` (the golden tags every ellipse) |
| `Location.X` / `Location.Y` | coord | Centre |
| `Radius` | coord | X radius |
| `SecondaryRadius` | coord | Y radius; defaults to `Radius` when absent (circle) |
| `LineWidth` | int | Border width index |
| `Color` / `AreaColor` | int | Border / fill colour (BGR; omit at 0) |
| `IsSolid` | bool | Filled; emit only when `T` |
| `Transparent` | bool | Emit only when `T` |

### Pie (RECORD=9)

A filled circular sector.

| Property | Type | Description |
|----------|------|-------------|
| `IsNotAccesible` | bool | Emit only when `T` |
| `Location.X` / `Location.Y` | coord | Centre |
| `Radius` | coord | Radius |
| `LineWidth` | int | Border width index (before the angles) |
| `StartAngle` / `EndAngle` | float | Degrees, 3-decimal form (`30.000`); `StartAngle` omitted at 0, `EndAngle` always written (defaults 0.0 / 360.0 on read) |
| `Color` / `AreaColor` | int | Border / fill colour (BGR; omit at 0) |
| `IsSolid` | bool | Filled; emit only when `T` |
| `Transparent` | bool | Emit only when `T` |

### RoundRectangle (RECORD=10)

| Property | Type | Description |
|----------|------|-------------|
| `Location.X` / `Location.Y` | coord | First corner |
| `Corner.X` / `Corner.Y` | coord | Second corner |
| `CornerXRadius` / `CornerYRadius` | coord | Corner radii |
| `LineWidth` | int | Border width index |
| `Color` / `AreaColor` | int | Border / fill colour (BGR; omit at 0) |
| `LineStyle` | int | 0=Solid, 1=Dashed, 2=Dotted; omit at 0 |
| `IsSolid` | bool | Filled; emit only when `T` |
| `Transparent` | bool | Emit only when `T` |

### EllipticalArc (RECORD=11)

| Property | Type | Description |
|----------|------|-------------|
| `Location.X` / `Location.Y` | coord | Centre |
| `Radius` (+ `Radius_Frac`) | coord | Primary radius |
| `SecondaryRadius` (+ `_Frac`) | coord | Secondary radius; defaults to `Radius` when absent |
| `LineWidth` | int | Line width (before the angles) |
| `StartAngle` / `EndAngle` | float | Degrees, 3-decimal form; `StartAngle` omitted at 0, `EndAngle` always written (defaults 0.0 / 360.0 = full ellipse on read) |
| `Color` / `AreaColor` | int | Line / fill colour (BGR; omit at 0) |

> **Note:** a fractional radius rounding up to a whole unit carries into the integer part rather
> than being clamped (see [Fractional coordinates](#fractional-coordinates)).

### Arc (RECORD=12)

| Property | Type | Description |
|----------|------|-------------|
| `IsNotAccesible` | bool | Emit only when `T` |
| `Location.X` / `Location.Y` | coord | Centre |
| `Radius` | coord | Arc radius |
| `LineWidth` | int | Line width (before the angles, per the ARCS golden) |
| `StartAngle` / `EndAngle` | float | Degrees, 3-decimal form (`EndAngle=360.000`); `StartAngle` omitted at 0, `EndAngle` always written (defaults 0.0 / 360.0 = full circle on read) |
| `Color` / `AreaColor` | int | Line / fill colour (BGR; omit at 0) |

### Line (RECORD=13)

| Property | Type | Description |
|----------|------|-------------|
| `IsNotAccesible` | bool | Emit only when `T` |
| `Location.X` / `Location.Y` | coord | Start point |
| `Corner.X` / `Corner.Y` | coord | End point |
| `LineWidth` | int | Line width index |
| `LineStyle` | int | 0=Solid, 1=Dashed, 2=Dotted; omit at 0 |
| `Color` | int | Line colour (BGR; omit at 0) |
| `LineStyleExt` | int | Style companion, same value as `LineStyle`; omit at 0 (a golden dashed line carries BOTH `LineStyle=1` and `LineStyleExt=1`; the reader accepts either) |

### Rectangle (RECORD=14)

| Property | Type | Description |
|----------|------|-------------|
| `IsNotAccesible` | bool | Always `T` on write |
| `Location.X` / `Location.Y` | coord | First corner |
| `Corner.X` / `Corner.Y` | coord | Second corner |
| `LineWidth` | int | Border line width index |
| `Color` / `AreaColor` | int | Border / fill colour (BGR; omit at 0) |
| `LineStyleExt` | int | Border style — rectangles store the line style in `LineStyleExt`, NOT `LineStyle`; omit at 0 |
| `IsSolid` | bool | Filled; emit only when `T` |
| `Transparent` | bool | Emit only when `T` (the golden's unfilled rectangle carries neither `IsSolid` nor `Transparent`) |

### TextFrame (RECORD=28)

A bordered multi-line text box. All keys below are omit-when-default (note the defaults of 0 for
`LineWidth` and `FontID`, unlike other shapes).

| Property | Type | Description |
|----------|------|-------------|
| `IsNotAccesible` | bool | Emit only when `T` |
| `Location.X` / `Location.Y` | coord | First corner |
| `Corner.X` / `Corner.Y` | coord | Second corner |
| `LineWidth` | int | Border width; omit at 0 |
| `Color` | int | Border colour (BGR; omit at 0) |
| `LineStyle` | int | Border style; omit at 0 |
| `AreaColor` | int | Fill colour (BGR; **always written**, even 0) |
| `TextColor` | int | Text colour (BGR; omit at 0) |
| `FontID` | int | Font reference (**always written**) |
| `IsSolid` / `ShowBorder` | bool | Emit only when `T` |
| `Orientation` | int | 0-3; omit at 0 |
| `Alignment` | int | Text alignment; omit at 0 |
| `WordWrap` / `ClipToRect` | bool | Emit only when `T` |
| `Text` / `%UTF8%Text` | string | Multi-line content (always written) |
| `TextMargin` (+ `_Frac`) | coord | Margin, following the omit-every-zero-key coordinate rule (a default frame carries only `TextMargin_Frac=5`) |
| `Transparent` | bool | Emit only when `T`, after `TextMargin` |

### Image (RECORD=30)

The picture metadata; embedded bytes live in [`/Storage`](#storage-embedded-images).

| Property | Type | Description |
|----------|------|-------------|
| `IsNotAccesible` | bool | Emit only when `T` |
| `Location.X` / `Location.Y` | coord | Bounding-box corner 1 |
| `Corner.X` / `Corner.Y` | coord | Bounding-box corner 2 |
| `LineWidth` | int | Border width index |
| `Color` | int | Border colour (BGR; omit at 0) |
| `LineStyle` | int | Border style; omit at 0 |
| `AreaColor` | int | Fill colour (BGR; omit at 0) |
| `IsSolid` / `Transparent` / `ShowBorder` | bool | Emit only when `T` |
| `KeepAspect` | bool | Preserve aspect ratio; emit only when `T` |
| `EmbedImage` | bool | `T` = bytes embedded in `/Storage` (matched in global stream order) |
| `FileName` | string | Image file path (also used as the `/Storage` entry name); omit when empty |

### Designator (RECORD=34)

A parameter-record variant selected by `Name=Designator`. As written by this crate:

| Property | Value |
|----------|-------|
| `IndexInSheet` / `OwnerPartId` | -1 / -1 (system record; no counter slot) |
| `Location.X` / `Location.Y` | Designator position, modelled on the symbol (golden default -5 / 5; zero keys omitted) |
| `Color` | 8388608 (dark blue) |
| `FontID` | 1 |
| `Text` / `%UTF8%Text` | Designator text (e.g. `R?`) |
| `Name` | `Designator` |
| `ReadOnlyState` | 1 |
| `UniqueID` | 8-char id, preserved from read (generated only when absent) |

### Parameter (RECORD=41)

| Property | Type | Description |
|----------|------|-------------|
| `IndexInSheet` | int | Shared content counter for **user** parameters (`OwnerPartId >= 1`, 0 omitted); the `-1` sentinel for **system** parameters (`OwnerPartId=-1`, no counter slot). Directly after `RECORD` (parameters carry no `IsNotAccesible` token) |
| `OwnerPartId` | int | Part ownership (-1 = system Comment-class record) |
| `Location.X` / `Location.Y` | coord | Position; every zero key omitted, `_Frac` companions adjacent to their integer keys |
| `Orientation` | int | 0-3; omit at 0 |
| `Justification` | int | Text anchor 0-8 (same table as Label); omit at 0 (golden JUSTIFY carries `Justification=8`/`=4`) |
| `Color` | int | Text colour (BGR; omit at 0 — the golden's user parameters carry no key) |
| `FontID` | int | Font reference (always written) |
| `IsHidden` | bool | Emit only when `T` |
| `Text` / `%UTF8%Text` | string | Parameter value; omit when empty |
| `Name` | string | Parameter name (always written) |
| `ReadOnlyState` | int | Omit at 0 (after `Name`, per real Altium output) |
| `ParamType` | int | 0=String, 1=Boolean, 2=Integer, 3=Float; omit at 0 |
| `ShowName` / `HideName` / `IsConfigurable` | bool | Emit only when `T` |
| `Description` | string | Omit when empty |

### Implementation chain (RECORD=44/45/46/47/48)

- **RECORD=44 (ImplementationList)** — always written, exactly `|RECORD=44`, even when the symbol
  has no footprint models.
- **RECORD=45 (Implementation)** — one per footprint model, owned by the RECORD=44 via
  `OwnerIndex` = the 44's 0-based stream-index:

  | Property | Type | Description |
  |----------|------|-------------|
  | `OwnerIndex` | int | Stream-index of the owning RECORD=44 |
  | `IndexInSheet` | int | -1 |
  | `Description` | string | Model description |
  | `ModelName` | string | Footprint name |
  | `ModelType` | string | `PCBLIB` (also `SIM` / `SI` in the wild) |
  | `DatafileCount` | int | Number of datafile links (this crate writes 1) |
  | `ModelDatafile0` | string | Optional `.PcbLib` path — what lets Altium resolve the footprint directly |
  | `ModelDatafileEntity0` | string | Footprint entity (resolution key) |
  | `ModelDatafileKind0` | string | `PCBLib` |
  | `IsCurrent` | bool | `T` on the default footprint |

- **RECORD=46 (MapDefinerList)** and **RECORD=48 (ImplementationParameters)** — written as empty
  children of each RECORD=45 (`|RECORD=46|OwnerIndex={45's index}` / `|RECORD=48|OwnerIndex=...`).
- **RECORD=47 (MapDefiner)** — pin-to-pad mapping (`DESINTF`, `DESIMPCOUNT`, `DESIMP{i}`,
  `ISTRIVIAL`); structure known from AltiumSharp but currently skipped on read and never written.

> **Note:** `DatafileCount=1` plus the `ModelDatafileEntity0` link is what lets Altium *resolve*
> the model to an actual footprint in a `PcbLib`; a name-only record with `DatafileCount=0` shows
> in the list but reports "model not found". AltiumSharp indexes the datafile keys 1-based
> (`MODELDATAFILEKIND1`); this crate writes 0-based, matching observed files — the index base is
> still under golden verification (TODO §B).

## Default Values

Read-side defaults when properties are absent:

| Property | Default | Notes |
|----------|---------|-------|
| `FontId` | 1 | Except TextFrame (0) |
| `StartAngle` / `EndAngle` | 0.0 / 360.0 | Arc, EllipticalArc, Pie |
| `OwnerPartId` | 1 | Shapes default to 1; -1 = all parts |
| `OwnerPartDisplayMode` | 0 | |
| `IndexInSheet` | positional | Shared 0-based content counter; slot 0 omitted; `-1` on header/system records |
| `LineWidth` | 1 | Except TextFrame (0) |
| `Color` / `AreaColor` | 0 (black) | Absent colour keys read as 0 on every record (Altium omits zero colours) |
| `SecondaryRadius` | = `Radius` | Ellipse, EllipticalArc |
| `PartCount` | stored − 1 | No floor at 1 |
| Booleans | false | Only `=T` is ever written |

## Symbol Writing Order

When writing symbol data, this crate encodes records in this specific order (the shared
`IndexInSheet` counter runs across steps 2-17; the designator and system parameters keep the
`-1` sentinel and consume no slot):

1. Component header (RECORD=1)
2. Rectangles (RECORD=14) — before the pins so a solid body does not paint over pin names
3. Pins (binary records; each consumes an `IndexInSheet` slot)
4. Lines (RECORD=13)
5. Polylines (RECORD=6)
6. Polygons (RECORD=7)
7. Arcs (RECORD=12)
8. Pies (RECORD=9)
9. Images (RECORD=30)
10. Text frames (RECORD=28)
11. Bezier curves (RECORD=5)
12. Ellipses (RECORD=8)
13. Rounded rectangles (RECORD=10)
14. Elliptical arcs (RECORD=11)
15. Labels (RECORD=4)
16. Text annotations (RECORD=3)
17. User parameters (RECORD=41, `OwnerPartId >= 1`) — after the graphic content, matching the
    golden stream order (JUSTIFY stores labels at slots 0-3, user parameters at 4-5)
18. Designator (RECORD=34, when non-empty)
19. System parameters (RECORD=41, `OwnerPartId = -1`) — after the designator, as the golden
    orders them
20. Implementation list (RECORD=44), then per footprint model: RECORD=45 + RECORD=46 + RECORD=48

The stream ends with the last record's payload — there is **no** trailing end marker (see the Data
Stream Format section and issue #68).

## Multi-Part Symbols

Some symbols have multiple parts (e.g. quad op-amp):

- `PartCount` in the component header indicates total parts (stored as count + 1)
- Each primitive has an `OwnerPartId` field:
    - `-1` = belongs to all parts
    - `1+` = belongs to a specific part

## Notes

- **Pin text format (RECORD=2)**: rare; skipped on read (binary pins are authoritative)
- **Pin symbol decorations**: supported (22 symbol types at 4 positions)
- **Display modes**: count in `DisplayModeCount`; primitives carry `OwnerPartDisplayMode`
- **Font storage**: fonts defined in FileHeader (`FontName{N}`, `Size{N}`)
- **Unique IDs**: all records carry an 8-char alphanumeric `UniqueID` (last key)
- **Embedded images**: `RECORD=30` metadata + zlib payloads in `/Storage`, order-matched

## References

- [AltiumSharp](https://github.com/issus/AltiumSharp) - C# library for Altium files (MIT)
- [pyAltiumLib](https://github.com/ChrisHoyer/pyAltiumLib) - Python library for reading Altium files
- [python-altium](https://github.com/vadmium/python-altium) - Altium format documentation
- Sample analysis: `scripts/analyse/analyse_schlib.py`
