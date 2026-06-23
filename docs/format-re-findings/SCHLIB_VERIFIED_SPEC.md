## SchLib Verified Field-by-Field Specification

All SchLib component primitives live in the component's OLE `Data` stream. Each record is framed as a 4-byte little-endian word — low 24 bits = payload length (**including** the trailing `0x00`), high byte = flags (`0x00` = ASCII parameter record, `0x01` = binary pin record). There is **no** `[0x00 0x00]` end-of-stream marker; records are read while bytes remain. Text payloads are Windows-1252, leading `|`, no trailing `|`, NUL-terminated. Zero-valued numeric keys and false booleans are **omitted** (AddNonZero / AddBool); a missing key means its default (numeric 0, bool false). `UniqueID` is always emitted **last**. Coordinates are DXP whole units (1 DXP = 10 mil = 100000 raw internal units); a paired `KEY_Frac` carries the signed remainder `raw % 100000` and is omitted when 0, so the full value is `KEY*100000 + KEY_Frac`. Colours are Win32 BGR (`0x00BBGGRR`).

### Common framing (all records)

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| +0 | 3 | u24 LE | BlockLength | Payload byte length incl. trailing NUL (low 24 bits of the size word) |
| +3 | 1 | u8 | Flags | `0x00` = text/parameter record, `0x01` = binary pin record |
| +4 | =len | bytes | Payload | NUL-terminated Windows-1252 `\|KEY=VALUE...` C-string (text), or binary struct (pin) |

### Common parameter keys (text records, omitted at default unless noted)

| Type | Field | Meaning |
|---|---|---|
| int | RECORD | Record-type discriminator (first key) |
| int | OwnerIndex | Owning record index; omitted in SchLib (positional ownership) |
| 'T' | IsNotAccesible | Not selectable (note Altium's single-'s' misspelling); omit when false |
| int | IndexInSheet | Stored z-order within the symbol; omit when 0 |
| int | OwnerPartId | Owning part (multi-part); omit when 0 |
| int | OwnerPartDisplayMode | Alternate display mode; omit when 0 |
| 'T' | GraphicallyLocked / Disabled / Dimmed | Primitive state flags; omit when false |
| string | UniqueID | 8-char per-primitive id, always **last** |

### sch:component (RECORD=1)

First record of each component's Data stream. Canonical key order from 103/103 golden components.

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| param | var | int | RECORD | =1 |
| param | var | string | LibReference | Component name |
| param | var | string | ComponentDescription | Free-text; OMITTED when empty |
| param | var | int | PartCount | Stored as user_parts + 1 (part 0 is shared); read = max(0, PartCount-1) |
| param | var | int | DisplayModeCount | Number of display modes (typically 1) |
| param | var | int | IndexInSheet | -1 for the component root |
| param | var | int | OwnerPartId | -1 for the component root |
| param | var | int | CurrentPartId | Currently displayed part (default 1) |
| param | var | string | LibraryPath | '*' sentinel = local |
| param | var | string | SourceLibraryName | '*' sentinel |
| param | var | string | SheetPartFileName | '*' sentinel |
| param | var | string | TargetFileName | '*' sentinel |
| param | var | string | UniqueID | Per-component id |
| param | var | int | AreaColor | Fill colour BGR (golden 11599871) |
| param | var | int | Color | Line colour BGR (golden 128) |
| param | var | bool | PartIDLocked | 'T'/'F' |
| param | var | int | AllPinCount | Total pins across all parts; emitted **last**; omit when 0 |

### sch:pin (RECORD=2, binary; flags byte = 0x01)

Fixed binary struct followed by Pascal short strings (`[u8 len][N bytes Windows-1252]`). Field order is strict; sub-unit fractions live in the separate `PinFrac` stream, per-pin line widths in `PinSymbolLineWidth`.

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| 0 | 4 | i32 LE | RecordType | =2 |
| 4 | 1 | u8 | Reserved (Unknown1) | Always 0x00 |
| 5 | 2 | i16 LE | OwnerPartId | 1-based; -1 = all parts |
| 7 | 1 | u8 | OwnerPartDisplayMode | Always written 0 |
| 8 | 1 | u8 | Symbol_InnerEdge | Inner-edge decoration enum |
| 9 | 1 | u8 | Symbol_OuterEdge | Outer-edge decoration enum |
| 10 | 1 | u8 | Symbol_Inside | Inside decoration enum |
| 11 | 1 | u8 | Symbol_Outside | Outside decoration enum |
| 12 | 1+N | Pascal | Description | Single Pascal short string (NO interior reserved byte) |
| 13+N | 1 | u8 | **FormalType** | Formal pin type (0 or 1) — comes AFTER Description, BEFORE Electrical |
| 14+N | 1 | u8 | Electrical | 0=Input,1=IO,2=Output,3=OpenCollector,4=Passive,5=HiZ,6=OpenEmitter,7=Power |
| 15+N | 1 | u8 | PinConglomerate | bits0-1 orientation; 0x04 Hidden; 0x08 ShowName; 0x10 ShowDesignator; 0x20 IsNotAccessible; 0x40 GraphicallyLocked |
| 16+N | 2 | i16 LE | PinLength | DXP units (integer part; fraction in PinFrac) |
| 18+N | 2 | i16 LE | Location.X | DXP units (integer part) |
| 20+N | 2 | i16 LE | Location.Y | DXP units (integer part) |
| 22+N | 4 | i32 LE | Color | Win32 BGR |
| 26+N | 1+M | Pascal | Name | Pin name |
| after | 1+K | Pascal | Designator | Pin designator |
| after | 1+P | Pascal | SwapIdGroup | Always empty in goldens |
| after | 1+Q | Pascal | PartAndSequence | `{SwapIdPart}\|&\|{SwapIdSequence}`; empty = no swap field |
| after | 1+R | Pascal | **DefaultValue** | Pin default value (e.g. '3.3V'); LAST field |

### sch:label (RECORD=4)

Text label. **RECORD=3 is Symbol (a graphic), NOT text.** Canonical key order: RECORD, IsNotAccesible, IndexInSheet, OwnerPartId, Location.X, Location.X_Frac, Location.Y, Location.Y_Frac, Orientation, Justification, Color, FontID, Text, [AreaColor, IsHidden, IsMirrored], UniqueID.

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| param | var | int | Location.X / Location.X_Frac | Anchor X (whole DXP + signed sub-unit); omit at 0 |
| param | var | int | Location.Y / Location.Y_Frac | Anchor Y; omit at 0 |
| param | var | int | Orientation | 0/1/2/3 = 0/90/180/270 deg; omit at 0 |
| param | var | int | Justification | 0=BL,1=BC,2=BR,3=ML,4=MC,5=MR,6=TL,7=TC,8=TR; omit at 0 |
| param | var | int | Color | Text colour BGR; omit at 0 (default 0/black) |
| param | var | int | FontID | 1-based font-table index (always written) |
| param | var | string | Text / %UTF8%Text | Label text (always written; %UTF8% for non-cp1252) |
| param | var | int | AreaColor | Background colour; omit at 0 |
| param | var | 'T' | IsHidden | Hidden; emit only when true (after Text) |
| param | var | 'T' | IsMirrored | Mirrored; emit only when true (after IsHidden) |

### sch:bezier (RECORD=5)

Cubic Bezier; 4 control points. Coordinates are **schematic units** (raw/1000, 10 units per mil), omitted when 0.

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| param | var | int | LineWidth | 0-3 width index (always written) |
| param | var | int | Color | Line colour BGR; omit at 0 |
| param | var | int | AreaColor | Fill colour; omit at 0 |
| param | var | int | LocationCount | Control-point count (=4) |
| param | var | int | X{n} / Y{n} | Control point n (1-indexed); omit when 0 |

### sch:polyline (RECORD=6)

Vertices `X{n}/Y{n}` are schematic units, omitted when 0. Canonical order: ..., LineWidth, LineStyle, StartLineShape, EndLineShape, LineShapeSize, Color, LocationCount, X/Y..., [LineStyleExt], UniqueID.

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| param | var | int | LineWidth | 0-3 index (always written) |
| param | var | int | LineStyle | 0=Solid,1=Dashed,2=Dotted; omit at 0 |
| param | var | int | StartLineShape / EndLineShape | End-shape enums; omit at 0 |
| param | var | int | LineShapeSize | End-shape size; omit at 0 |
| param | var | int | Color | Line colour BGR; omit at 0 |
| param | var | int | LocationCount | Vertex count |
| param | var | int | X{n} / Y{n} | Vertex n (1-indexed); omit when 0 |
| param | var | int | LineStyleExt | = LineStyle, written after vertices when LineStyle!=0 |

### sch:polygon (RECORD=7)

Filled polygon. Canonical order: RECORD, IsNotAccesible, IndexInSheet, OwnerPartId, LineWidth, [Color], AreaColor, [IsSolid], LocationCount, vertices, UniqueID.

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| param | var | int | LineWidth | Border width index (always written) |
| param | var | int | Color | Border colour BGR; omit at 0 |
| param | var | int | AreaColor | Fill colour BGR (**always written**) |
| param | var | 'T' | IsSolid | Whether FILLED; emit only when true (absent = unfilled) |
| param | var | int | LocationCount | Vertex count |
| param | var | int | X{n} / Y{n} | Vertex n (1-indexed); omit when 0 |

### sch:ellipse (RECORD=8)

Ellipse/circle. SecondaryRadius defaults to Radius when absent (circle).

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| param | var | int | Location.X / Location.X_Frac | Centre X (DXP + sub-unit); omit at 0 |
| param | var | int | Location.Y / Location.Y_Frac | Centre Y; omit at 0 |
| param | var | int | Radius / Radius_Frac | Primary (X) radius; omit at 0 |
| param | var | int | SecondaryRadius / _Frac | Secondary (Y) radius; defaults to Radius |
| param | var | int | LineWidth | Border width index (always written) |
| param | var | int | Color | Border colour BGR; omit at 0 (default 0) |
| param | var | int | AreaColor | Fill colour BGR (always written) |
| param | var | 'T' | IsSolid | Filled; emit only when true |
| param | var | 'T' | Transparent | Transparent fill; emit only when true |

### sch:roundrect (RECORD=10)

Rounded rectangle. Two opposite corners + per-axis corner radii.

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| param | var | int | Location.X/Y (+_Frac) | First corner |
| param | var | int | Corner.X/Y (+_Frac) | Second corner |
| param | var | int | CornerXRadius / _Frac | Horizontal corner radius |
| param | var | int | CornerYRadius / _Frac | Vertical corner radius |
| param | var | int | LineWidth | Width index (always written) |
| param | var | int | LineStyle | 0=Solid,1=Dashed,2=Dotted,3=DashDot; omit at 0 |
| param | var | int | Color | Border colour BGR; omit at 0 |
| param | var | int | AreaColor | Fill colour BGR (always written) |
| param | var | 'T' | IsSolid | Filled; absent = unfilled |
| param | var | 'T' | Transparent | Transparent fill; emit only when true |

### sch:ellipticalarc (RECORD=11)

Elliptical arc. Canonical order: RECORD, IsNotAccesible, IndexInSheet, OwnerPartId, Location.X, Location.Y, Radius, SecondaryRadius, LineWidth, [StartAngle], EndAngle, Color, UniqueID. Angles formatted with 3 decimals ('F3').

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| param | var | int | Location.X/Y (+_Frac) | Centre; omit at 0 |
| param | var | int | Radius / Radius_Frac | Primary radius |
| param | var | int | SecondaryRadius / _Frac | Secondary radius |
| param | var | int | LineWidth | Stored as a Coord here (not an enum index) |
| param | var | f64 | StartAngle | Degrees, F3 format ('45.000'); OMIT when 0 |
| param | var | f64 | EndAngle | Degrees, F3 format; always written |
| param | var | int | Color | Line colour BGR; omit at 0 |
| param | var | int | AreaColor | Fill colour BGR; omit at 0 |

### sch:arc (RECORD=12)

Circular arc. Canonical order: RECORD, IsNotAccesible, IndexInSheet, OwnerPartId, Location.X, Location.X_Frac, Location.Y, Radius, Radius_Frac, LineWidth, [StartAngle], EndAngle, Color, AreaColor, UniqueID.

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| param | var | int | Location.X/Y (+_Frac, signed) | Centre; omit at 0 (default 0) |
| param | var | int | Radius / Radius_Frac (signed) | Radius (whole DXP + sub-unit) |
| param | var | int | LineWidth | 0-3 width index (always written) |
| param | var | f64 | StartAngle | Degrees CCW, F3 format; OMIT when 0 |
| param | var | f64 | EndAngle | Degrees CCW, F3 format; always written |
| param | var | int | Color | Line colour BGR (decimal); omit at 0 (default 0) |
| param | var | int | AreaColor | Fill colour; omit at 0 |

### sch:line (RECORD=13)

Open line segment. Canonical order: RECORD, IsNotAccesible, IndexInSheet, OwnerPartId, Location.X/Y, Corner.X/Y, LineWidth, [Color], [LineStyleExt], [AreaColor], UniqueID.

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| param | var | int | Location.X/Y (+_Frac) | Start point; omit at 0 |
| param | var | int | Corner.X/Y (+_Frac) | End point; omit at 0 |
| param | var | int | LineWidth | 0-3 width index; omit at 0 |
| param | var | int | LineStyle | 0=Solid,1=Dashed,2=Dotted; omit at 0 |
| param | var | int | Color | Line colour BGR; omit at 0 |
| param | var | int | LineStyleExt | = LineStyle, written after Color when LineStyle!=0 (write-only) |
| param | var | int | AreaColor | Fill colour; omit at 0 |

### sch:rectangle (RECORD=14)

Two-corner rectangle. Canonical order: RECORD, IsNotAccesible, IndexInSheet, OwnerPartId, Location.X/Y, Corner.X/Y, [LineStyleExt], LineWidth, [Color], [AreaColor], [IsSolid], [Transparent], UniqueID.

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| param | var | int | Location.X/Y (+_Frac) | First corner; omit at 0 |
| param | var | int | Corner.X/Y (+_Frac) | Second corner; omit at 0 |
| param | var | int | LineStyleExt | Border style 0=Solid,1=Dashed,2=Dotted,3=DashDot (before LineWidth); omit at 0 |
| param | var | int | LineWidth | 0-3 enum index; omit at 0 (Small) |
| param | var | int | Color | Border colour BGR; omit at 0 (default 0) |
| param | var | int | AreaColor | Fill colour BGR (default 11599871); omit at 0 |
| param | var | 'T' | IsSolid | Filled; emit only when true (absent = unfilled) |
| param | var | 'T' | Transparent | Transparent fill; emit only when true |

### sch:designator / sch:parameter (RECORD=34 / RECORD=41)

Both share one field set (SchParameterDto, `[AltiumRecord("41")]`); RECORD=34 is selected when `Name=="Designator"`, RECORD=41 otherwise (incl. Comment/Value). ~39 possible keys; only the common subset appears in practice. Canonical order: RECORD, IndexInSheet, OwnerPartId, Location.X (+_Frac), Location.Y (+_Frac), Color, FontID, [IsHidden], Text/%UTF8%Text, Name, [ReadOnlyState], [ParamType], ..., UniqueID (last).

| Offset | Size | Type | Field | Meaning |
|---|---|---|---|---|
| param | var | int | RECORD | 34 (designator) or 41 (parameter) |
| param | var | int | IndexInSheet | -1 for component-level designator/comment |
| param | var | int | OwnerPartId | -1 for designator/comment |
| param | var | int | Location.X / _Frac | Text X (DXP + sub-unit); omit at 0 |
| param | var | int | Location.Y / _Frac | Text Y |
| param | var | int | Color | Text colour BGR (designator 8388608) |
| param | var | int | FontID | 1-based font index |
| param | var | 'T' | IsHidden | Hidden; emit only when true (never 'F') |
| param | var | string | Text / %UTF8%Text | Value/designator text; OMITTED when empty |
| param | var | string | Name | 'Designator', 'Comment', 'Value', etc. (after Text) |
| param | var | int | ReadOnlyState | Read-only flag; emit only when set, AFTER Name |
| param | var | int | ParamType | 0=String,1=Bool,2=Int,3=Float; omit at 0, after ReadOnlyState |

### sch:implementation family (RECORD=44/45/46/47/48)

Footprint/model link records, ASCII parameter blocks. **Correct meanings:** 44=ImplementationList (container, SchLib emits empty `\|RECORD=44`), 45=Implementation (model link), 46=MapDefinerList (container), 47=MapDefiner (pin mapping), 48=ImplementationParameters (container). In the SchLib path ownership is positional (no OwnerIndex); the SchDoc path adds OWNERINDEX.

| Record | Type | Field | Meaning |
|---|---|---|---|
| 45 | string | Description | Model description; emit when non-empty |
| 45 | string | ModelName | Footprint/model name |
| 45 | string | ModelType | PCBLIB / SIM / SI |
| 45 | int | DataFileCount | Number of Kind/Entity pairs |
| 45 | string | MODELDATAFILEKIND{i} | Data-file kind (e.g. 'PCBLib'); **1-based** index per AltiumSharp |
| 45 | string | MODELDATAFILEENTITY{i} | Footprint entity (resolution key); **1-based** |
| 45 | 'T' | ISCURRENT | Active model; emit only when true |
| 45 | string | UniqueID | Per-record id (last) |
| 47 | string | DESINTF | Schematic designator-interface name |
| 47 | int | DESIMPCOUNT | Count of DESIMP{i} |
| 47 | string | DESIMP{i} | Implementation-side designator (**0-based**) |
| 47 | 'T' | ISTRIVIAL | Trivial 1:1 mapping; emit only when true |