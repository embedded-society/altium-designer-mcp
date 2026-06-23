## PcbLib Verified Field Reference

All multi-byte integers are little-endian. Text is Windows-1252. Coordinates are raw i32 Altium internal units where 1 unit = 1/10000 mil = 2.54 nm (10000 units = 1 mil). Each primitive in a footprint `/{component}/Data` stream is `[u8 objectId][u32 LE blockLen (low 24 bits; high byte = flags, mask 0x00FFFFFF)][block body]`. There is **no** trailing end marker between primitives.

### Common Header (first 13 bytes of every primitive block)

| Offset | Size | Type | Field | Meaning |
|--------|------|------|-------|---------|
| 0 | 1 | u8 | Layer | Altium layer id (see Layer IDs) |
| 1 | 2 | u16 | Flags | On-wire flag word (see PcbFlags) |
| 3 | 2 | u16 | NetIndex | 0xFFFF = no net (always 0xFFFF in libraries) |
| 5 | 2 | u16 | PolygonIndex | 0xFFFF = none |
| 7 | 2 | u16 | ComponentIndex | 0xFFFF = free primitive (→ -1) |
| 9 | 4 | u32 | Reserved | 0xFFFFFFFF |

**PcbFlags (on-wire u16):** 0x04 Unlocked (inverted — clear = locked); 0x08 Saved (always set on saved primitives); 0x20 TentingTop; 0x40 TentingBottom; 0x200 Keepout. Saved library primitives carry ≥0x000C; keepout primitives carry 0x020C. (These differ from the project's internal abstract `PcbFlags` enum.)

### Layer IDs (offset-0 byte)

1 Top; 2–31 Mid1–30; 32 Bottom; 33/34 Top/Bottom Overlay; 35/36 Top/Bottom Paste; 37/38 Top/Bottom Solder; 39–54 Internal Plane 1–16; 55 Drill Guide; 56 Keep-Out; 57–72 Mechanical 1–16; 73 Drill Drawing; 74 Multi-Layer; 81 Pad Hole; 82 Via Hole. (Mechanical 2–7 are aliased in our model as Assembly/Courtyard/3D-Body pairs but emit plain bytes 58–63.) 0 = No Layer.

### Arc (0x01) — 60-byte block

| Offset | Size | Type | Field | Meaning |
|--------|------|------|-------|---------|
| 0–12 | 13 | — | Common header | layer/flags/net/poly/comp/reserved |
| 13 | 4 | i32 | CentreX | |
| 17 | 4 | i32 | CentreY | |
| 21 | 4 | i32 | Radius | |
| 25 | 8 | f64 | StartAngle | degrees |
| 33 | 8 | f64 | EndAngle | degrees |
| 41 | 4 | i32 | Width | stroke width |
| 45 | 2 | i16 | SubPolyIndex | constant 0 |
| 47 | 4 | i32 | SolderMaskExpansion | |
| 51 | 1 | u8 | PasteMaskExpansion | 1-byte (cf. Track 2-byte) |
| 52 | 4 | u32 | V7LayerId | derived from layer |
| 56 | 1 | u8 | KeepoutRestrictions | |
| 57 | 3 | u8×3 | Reserved | 00 00 00 |

### Pad (0x02) — five sub-blocks

Sub-blocks: Block0 designator (size-prefixed Pascal string); Block1 reserved empty; Block2 net string default `|&|0`; Block3 reserved empty; Block4 main geometry (202 bytes in PcbLib); Block5 size/shape (0 for simple pads, else ≥596 bytes).

**Block4 main geometry (SR5):**

| Offset | Size | Type | Field | Meaning |
|--------|------|------|-------|---------|
| 0–12 | 13 | — | Common header | |
| 13 | 4 | i32 | LocationX | |
| 17 | 4 | i32 | LocationY | |
| 21 | 4 | i32 | SizeTopX | |
| 25 | 4 | i32 | SizeTopY | |
| 29 | 4 | i32 | SizeMiddleX | |
| 33 | 4 | i32 | SizeMiddleY | |
| 37 | 4 | i32 | SizeBottomX | |
| 41 | 4 | i32 | SizeBottomY | |
| 45 | 4 | i32 | HoleSize | 0 for SMD |
| 49 | 1 | u8 | ShapeTop | 1=Round,2=Rect,3=Octagonal,9=RoundedRect |
| 50 | 1 | u8 | ShapeMiddle | |
| 51 | 1 | u8 | ShapeBottom | |
| 52 | 8 | f64 | Rotation | degrees |
| 60 | 1 | u8 | IsPlated | independent bool, default 1 |
| 61 | 1 | u8 | (reserved, extended-tail start) | 0x00 — NOT hole shape |
| 62 | 1 | u8 | StackMode | 0=Simple,1=TopMiddleBottom,2=FullStack |
| 67 | 1 | u8 | PowerPlaneConnectStyle | |
| 68 | 4 | i32 | ReliefConductorWidth | |
| 72 | 2 | i16 | ReliefEntries | default 4 |
| 74 | 4 | i32 | ReliefAirGap | |
| 78 | 4 | i32 | PowerPlaneReliefExpansion | |
| 82 | 4 | i32 | PowerPlaneClearance | |
| 86 | 4 | i32 | PasteMaskExpansion | |
| 90 | 4 | i32 | SolderMaskExpansion | template default 40000 (0x9C40, 4 mil) |
| 101 | 1 | u8 | PasteMaskExpansionMode | 0=None,1=FromRule,2=Manual |
| 102 | 1 | u8 | SolderMaskExpansionMode | 0=None,1=FromRule,2=Manual |
| 110 | 2 | i16 | JumperId | |
| 114 | 4 | u32 | V7LayerId | derived |
| 121 | 4 | i32 | SolderMaskCache | mirrors +90 |
| 126 | 16 | GUID | IdentityGuid (per-pad) | |
| 142 | 16 | GUID | IdentityGuidB (pad-stack) | |
| 162 | 4 | i32 | HolePositiveTolerance | 0x7FFFFFFF = unset |
| 166 | 4 | i32 | HoleNegativeTolerance | 0x7FFFFFFF = unset |
| 172 | 1 | u8 | Format marker | 0x1A PcbLib / 0x12 PcbDoc |
| 185 | 1 | u8 | Reserved marker | default 0x03 |

**Block5 size/shape (≥596 bytes when present):** 29×i32 X-sizes @0; 29×i32 Y-sizes @116; 29×u8 internal shapes @232; reserved @261; u8 HoleType @262 (0=Round,1=Square,2=Slot); i32 HoleSlotLength @263; f64 HoleRotation @267; 32×i32 OffsetX-from-hole @275; 32×i32 OffsetY-from-hole @403; u8 HasRoundedRect @531; 32×u8 PerLayerShapes @532; 32×u8 PerLayerCornerRadii @564. Full-stack tail @596: `[32 reserved][u32 count][u32 stride=15][count×15-byte entry]`; block length = 636 + count×15. RoundedRectangle is stored directly as shape id 9 (gated by HasRoundedRect@531), not id 1 + radius.

### Via (0x03) — single 321-byte SubRecord-1

| Offset | Size | Type | Field | Meaning |
|--------|------|------|-------|---------|
| 0–12 | 13 | — | Common header | |
| 13 | 4 | i32 | X | |
| 17 | 4 | i32 | Y | |
| 21 | 4 | i32 | Diameter | |
| 25 | 4 | i32 | HoleSize | |
| 29 | 1 | u8 | StartLayer | |
| 30 | 1 | u8 | EndLayer | |
| 31 | 1 | u8 | PowerPlaneConnectStyle | |
| 32 | 4 | i32 | ThermalReliefAirGap | |
| 36 | 1 | u8 | ThermalReliefConductors | |
| 38 | 4 | i32 | ThermalReliefConductorsWidth | |
| 42 | 4 | i32 | PowerPlaneReliefExpansion | |
| 46 | 4 | i32 | PowerPlaneClearance | |
| 50 | 4 | i32 | PasteMaskExpansion | |
| 54 | 4 | i32 | SolderMaskExpansion (front) | template 40000 |
| 61 | 4 | i32 | CacheValidWord | golden 0 |
| 66 | 1 | u8 | SolderMaskExpansionMode | 0=None,1=FromRule,2=Manual |
| 67 | 1 | u8 | CacheValidByte | golden 0 |
| 70 | 1 | u8 | Reserved | |
| 72 | 1 | u8 | Reserved flag | |
| 74 | 1 | u8 | DiameterStackMode | 0=Simple,1=TMB,2=FullStack |
| 75 | 128 | 32×i32 | PerLayerDiameters | |
| 242 | 4 | i32 | SolderMaskExpansion (back) | |
| 258 | 1 | u8 | SolderMaskFromHoleEdge | |
| 259 | 16 | GUID | IdentityGuid (per-via) | |
| 275 | 16 | GUID | IdentityGuidB (per-footprint) | |
| 291 | 4 | i32 | HolePositiveTolerance | 0x7FFFFFFF = unset |
| 295 | 4 | i32 | HoleNegativeTolerance | 0x7FFFFFFF = unset |
| 312 | 1 | u8 | DrillLayerPairType | 0=Through,1/2/3 blind/buried |
| 320 | 1 | u8 | Trailing constant | 0x01 |

(Undecoded constant regions: 203–241, 254–255=0x002A, 304–307=30, 308–311=9 — replayed verbatim.)

### Track (0x04) — 49-byte block

| Offset | Size | Type | Field | Meaning |
|--------|------|------|-------|---------|
| 0–12 | 13 | — | Common header | |
| 13 | 4 | i32 | StartX | |
| 17 | 4 | i32 | StartY | |
| 21 | 4 | i32 | EndX | |
| 25 | 4 | i32 | EndY | |
| 29 | 4 | i32 | Width | |
| 33 | 2 | i16 | SubPolyIndex | constant 0 |
| 35 | 4 | i32 | SolderMaskExpansion | |
| 39 | 2 | i16 | PasteMaskExpansion | 2-byte (cf. Arc 1-byte) |
| 41 | 4 | u32 | V7LayerId | derived |
| 45 | 1 | u8 | KeepoutRestrictions | |
| 46 | 3 | u8×3 | Reserved | 00 00 00 |

### Text (0x05) — fixed 252-byte SubRecord-1 + ASCII SubRecord-2

| Offset | Size | Type | Field | Meaning |
|--------|------|------|-------|---------|
| 0–12 | 13 | — | Common header | |
| 13 | 4 | i32 | LocationX | |
| 17 | 4 | i32 | LocationY | |
| 21 | 4 | i32 | Height | |
| 25 | 2 | i16 | FontId | raw font-table index |
| 27 | 8 | f64 | Rotation | degrees |
| 35 | 1 | u8 | Mirrored | |
| 36 | 4 | i32 | StrokeWidth | |
| 40 | 1 | u8 | IsComment | |
| 41 | 1 | u8 | IsDesignator | |
| 42 | 1 | u8 | CharSet | |
| 43 | 1 | u8 | BaseFontType | 0=stroke,1=TrueType |
| 44 | 1 | u8 | Bold | |
| 45 | 1 | u8 | Italic | |
| 46–109 | 64 | UTF-16LE | FontName | NUL-terminated, default 'Arial' |
| 110 | 1 | u8 | IsInverted | |
| 111 | 4 | i32 | InvertedBorderWidth | |
| 115 | 4 | i32 | WideStringIndex | -1 = none |
| 119 | 4 | i32 | UnionIndex | |
| 123 | 1 | u8 | UseInvertedRect | |
| 124 | 4 | i32 | InvertedRectWidth | |
| 128 | 4 | i32 | InvertedRectHeight | |
| 132 | 1 | u8 | Justification | PCB column-major: 0=Manual,1=LeftTop..5=CentreCentre..9=RightBottom |
| 133 | 4 | i32 | InvertedRectTextOffset | |
| 137–156 | 20 | i32×5 | Barcode full/margins/minwidth | |
| 157 | 1 | u8 | BarcodeKind | |
| 158 | 1 | u8 | BarcodeRenderMode | |
| 159 | 1 | u8 | BarcodeInverted | |
| 160 | 1 | u8 | TextKind (authoritative) | 0=Stroke,1=TrueType,2=BarCode |
| 161–224 | 64 | UTF-16LE | BarcodeFontName | |
| 225 | 1 | u8 | BarcodeShowText | |
| 226 | 4 | u32 | V7LayerId | derived |
| 230 | 1 | u8 | IsFrame | |
| 231 | 1 | u8 | IsOffsetBorder | |
| 240 | 1 | u8 | IsJustificationValid | |
| 241 | 1 | u8 | AdvanceSnapping | |
| 244 | 4 | i32 | SnapPointX | |
| 248 | 4 | i32 | SnapPointY | |

Followed by SubRecord-2: `[u32 len][u8 strlen][Win1252 text]` (the text content or special string e.g. `.Designator`). Stroke font ids: 1=Default, 2=Sans-Serif, 3=Serif. UTF-16 content is held in `/{component}/WideStrings` at `WideStringIndex` (a plain 0-based array index; no reserved slots).

### Fill (0x06) — 50-byte block

| Offset | Size | Type | Field | Meaning |
|--------|------|------|-------|---------|
| 0–12 | 13 | — | Common header | |
| 13 | 4 | i32 | Corner1X | |
| 17 | 4 | i32 | Corner1Y | |
| 21 | 4 | i32 | Corner2X | |
| 25 | 4 | i32 | Corner2Y | |
| 29 | 8 | f64 | Rotation | degrees |
| 37 | 4 | i32 | SolderMaskExpansion | |
| 41 | 1 | u8 | PasteMaskExpansion | |
| 42 | 4 | u32 | V7LayerId | layer-derived (non-zero) |
| 46 | 1 | u8 | KeepoutRestrictions | |
| 47 | 3 | u8×3 | Reserved | 00 00 00 |

### Region (0x0B) — single block

| Offset | Size | Type | Field | Meaning |
|--------|------|------|-------|---------|
| 0–12 | 13 | — | Common header | |
| 13 | 1 | u8 | Reserved | 0x00 |
| 14 | 2 | u16 | HoleCount | number of hole contours |
| 16 | 2 | u8×2 | Reserved | 00 00 |
| 18 | 4 | u32 | ParamBlockLen | includes trailing NUL |
| 22 | var | C-string | ParamString | NUL-terminated, NO leading pipe |
| +0 | 4 | u32 | OutlineVertexCount | |
| +4 | 16×n | f64 pairs | OutlineVertices | X,Y doubles |
| … | per hole | `[u32 count][count×(f64 X,f64 Y)]` | HoleContours | repeated HoleCount times |

Param keys (canonical order, no leading pipe): `V7_LAYER=…|NAME= |KIND=0|SUBPOLYINDEX=-1|UNIONINDEX=0|ARCRESOLUTION=0.5mil|ISSHAPEBASED=FALSE|CAVITYHEIGHT=0mil` (append `KEEPOUTRESTRICTIONS` on keepout regions). NAME defaults to a single space. KIND 0=copper/standard, 1=cutout.

### ComponentBody (0x0C) — single block

| Offset | Size | Type | Field | Meaning |
|--------|------|------|-------|---------|
| 0–12 | 13 | — | Common header | |
| 13 | 4 | u32 | Reserved | 0 |
| 17 | 1 | u8 | Reserved | 0 |
| 18 | 4 | u32 | ParamBlockLen | includes NUL |
| 22 | var | C-string | ParamString | NO leading pipe |
| +0 | 4 | u32 | OutlineVertexCount | |
| +4 | 16×n | f64 pairs | OutlineVertices | X,Y doubles (no hole arrays) |

Param keys (canonical order): `V7_LAYER=MECHANICAL1|NAME= |KIND|SUBPOLYINDEX|UNIONINDEX|ARCRESOLUTION|ISSHAPEBASED|CAVITYHEIGHT|STANDOFFHEIGHT|OVERALLHEIGHT|BODYPROJECTION|ARCRESOLUTION(dup)|BODYCOLOR3D|BODYOPACITY3D|IDENTIFIER|TEXTURE…|MODELID|MODEL.CHECKSUM|MODEL.EMBED|MODEL.NAME|MODEL.2D.*|MODEL.3D.ROTX/Y/Z|MODEL.3D.DZ|MODEL.MODELTYPE|[MODEL.MODELSOURCE]|[MODEL.EXTRUDED.MINZ/MAXZ]|…`. MODELTYPE 0=extruded (default for footprint bodies), 1=embedded STEP. MODELSOURCE and EXTRUDED.MINZ/MAXZ are conditional. Mil-coords use `FormatMilCoord`; TEXTUREROTATION uses Delphi-exp format. Heights are mil-suffixed. Default 3D body layer is MECHANICAL1 (byte 57).

### OLE Stream Layout

**FileHeader (53 bytes):** `[u32 27][u8 27]["PCB 6.0 Binary Library File"][f64 5.01][u32 8][u8 8][8-char UniqueId]`.

**SectionKeys** (root, only when a footprint name >31 chars): `[u32 count]` then per entry `[Pascal full-name block][StringBlock 31-char OLE key]`.

**/Library/Header** = u32 record count (1). **/Library/Data** = `[u32 len][Win1252 '|KEY=VAL…'+NUL]` (FILENAME, KIND, VERSION=3.00, V9 layer stack) then `[u32 componentCount]` then per-footprint `[StringBlock OLE name]`. Sub-storages: EmbeddedFonts (u32=0), LayerKindMapping (`[u32 textLen][UTF-16 '1.0'+NUL][u32 sig=0][u32 count=0]`), PadViaLibrary, ComponentParamsTOC, Textures/ModelsNoEmbed (empty), Models.

**/{component}/Header** = u32 primitive count. **/Parameters** = `[u32 len]['|PATTERN=…|HEIGHT=0mil|DESCRIPTION=…|ITEMGUID=|REVISIONGUID='+NUL]`. **/Data** = `[StringBlock patternName]` then primitive records (no end marker). **/WideStrings** = `[u32 len]['|ENCODEDTEXT0=…'+NUL]` (leading pipe, NO trailing pipe; empty = blockLen 1 = just NUL). **/UniqueIDPrimitiveInformation/Header** = u32 count, **/Data** = per primitive `[u32 len]['|PRIMITIVEINDEX=N|PRIMITIVEOBJECTID=…|UNIQUEID=…'+NUL]` (PRIMITIVEINDEX is 1-based). **/PrimitiveGuids/Header** = u32 count, **/Data** = N×`[u32 typeId][u32 index][16-byte GUID]` (first record typeId=85 component, then typeId 1–12 per primitive).

**/FileVersionInfo/Header** = u32 1, **/Data** = `[u32 len]['|KEY=VAL…'+NUL]`.