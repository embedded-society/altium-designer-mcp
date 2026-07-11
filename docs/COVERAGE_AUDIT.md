# MCP Feature-Coverage Audit

<!-- markdownlint-disable MD013 MD037 MD049 -->

> Snapshot of every PcbLib + SchLib primitive feature our MCP reads / writes / exposes, versus what
> Altium actually stores (cross-checked against AltiumSharp and the committed goldens). Produced by the
> coverage-audit workflow. The roadmap below is the execution plan, worked PR by PR; prune as items ship.

## 1. HEADLINE

**PcbLib (~55% feature-complete):** Core geometry round-trips cleanly for every primitive, but the *write tool layer* is the dominant blocker — Vias and Fills can't be
created at all, and almost no primitive exposes its flags/mask/keepout fields on write. The biggest *format* hole is thermal-relief / power-plane connection on Pads and
Vias (entirely unmodelled), plus slot-hole geometry.

**SchLib (~50% feature-complete):** Reader/writer handle the visual core of every primitive, but **four primitives (RoundRect, Polygon, Ellipse, Arc, Label) are missing
from the write schema entirely** and a large set of universal Altium flags (OwnerPartDisplayMode, GraphicallyLocked, Disabled, Dimmed, *_FRAC sub-coords) are unmodelled
across *every* SchLib shape. Pin is the best-covered primitive but still can't author descriptions, swap groups, or open-collector/Hi-Z types via the tool.

---

## 2. FEATURE-LOSS GaPS (highest priority — "all features covered")

### PR-1 — SchLib: make all shape primitives writable (schema + handler)

**Primitives:** RoundRect, Polygon, Ellipse, SchArc, Label. **Kind:** tool. **Missing:** entire `round_rects` / `polygons` / `ellipses` / `arcs` / `labels` arrays absent
from `write_schlib` schema (and Polygon/RoundRect have no handler branch at all).
**Fix:** Add an items schema for each under the symbol object in `tool_definitions.rs`; add `parse_schlib_polygon` + `symbol.add_polygon`/`add_round_rect` calls in
`read_write.rs::call_write_schlib`. Ellipse/Arc/Label handlers exist but are undiscoverable — just add their schema entries. *Highest value: 4-5 primitives are currently
read-only.*

### PR-2 — PcbLib: make Via writable via write_pcblib

**Primitive:** Via. **Kind:** tool. **Missing:** no `vias` property in the footprint schema; `call_write_pcblib` never reads `fp_json.get("vias")`.
**Fix:** Add `vias` array (x, y, diameter, hole_size, from/to_layer, mask/thermal fields) to schema; parse it in the handler like pads/tracks. Model+reader+writer already
support Via fully.

### PR-3 — PcbLib: make Fill writable via write_pcblib

**Primitive:** Fill. **Kind:** tool. **Missing:** no `fills` array in create schema (only `update_primitive` can mutate an existing fill).
**Fix:** Add `fills` array (x1/y1/x2/y2, rotation, layer, solder_mask_expansion, keepout_restrictions, flags) to the footprint schema; parse it in `call_write_pcblib` and
call `footprint.add_fill`.

### PR-4 — PcbLib: expose flags / mask / keepout on write for all 2D primitives

**Primitives:** Pad, Track, Arc, Region, Fill, Text. **Kind:** tool. **Missing:** `flags` (locked/keepout/tenting), `solder_mask_expansion`, `keepout_restrictions` — all
read+written+round-tripped but hard-coded to empty/None in the `parse_*` functions.
**Fix:** Add these keys to each primitive's `write_pcblib` item schema and read them in `parsing.rs` instead of hard-coding `PcbFlags::empty()` / `None`. Pure tool-layer
change, no format work.

### PR-5 — PcbLib: Pad/Via per-layer stack + documented optional fields on write

**Primitive:** Pad (and Via stack). **Kind:** tool. **Missing:** `stack_mode`, `per_layer_sizes/shapes/corner_radii/offsets` hard-coded to Simple/None in `parse_pad`;
plus `rotation`/`hole_shape`/`*_mask_expansion`/`*_mode`/`corner_radius_percent` are accepted but undocumented in schema.
**Fix:** Add the per-layer keys + document the already-accepted keys in `tool_definitions.rs`; stop hard-coding stack_mode/per_layer in `parse_pad`.

### PR-6 — PcbLib: Pad thermal-relief & power-plane connection (format)

**Primitive:** Pad. **Kind:** read+write+tool. **Missing:** PowerPlaneConnectStyle (off.67), ReliefConductorWidth (68), ReliefEntries (72), ReliefAirGap (74),
PowerPlaneReliefExpansion (78), PowerPlaneClearance (82).
**Fix:** Add 6 fields to the Pad struct; read them in `parse_pad` at the listed extended-tail offsets; write them in `build_pad_extended_tail` instead of template
constants; expose in tool schema. (Via already models thermal relief — mirror it.)

### PR-7 — PcbLib: Via thermal-relief, power-plane, tenting, net flags (format)

**Primitive:** Via. **Kind:** read+write+tool. **Missing:** IsTentingTop/Bottom, IsKeepout, IsLocked (flags word @1-2), PowerPlaneConnectStyle (@31),
PowerPlaneReliefExpansion (@42), PowerPlaneClearance (@46), PasteMaskExpansion (@50), Net/NetIndex (@3-4).
**Fix:** Decode the flags word and these offsets in `parse_via`; add struct fields; write them in `encode_via`; expose tenting/keepout/thermal/paste in the (new from
PR-2) via schema. Tenting is the highest-value EE property here.

### PR-8 — PcbLib: Pad/Via slot-hole geometry & drill tolerances (format)

**Primitives:** Pad, Via. **Kind:** read+write+tool. **Missing:** HoleSlotLength + HoleRotation (currently hard-coded 0 in writer),
HolePositiveTolerance/HoleNegativeTolerance (hard-coded 0x7FFFFFFF sentinel).
**Fix:** Read slot length/rotation from the size-shape block and tolerances from extended-tail offsets 162/166 (Pad) / 291/295 (Via); add struct fields; replace the
hard-coded writer values; expose in schema. Without this `hole_shape='slot'` produces a zero-length degenerate slot.
**Deferred:** DrillType (simple/pressfit) — the `.PcbLib` binary has no verified offset (neither AltiumSharp nor pyaltiumlib serialise it, and every pad in AltiumSharp's
test corpus is `Simple`). Deferred until a press-fit golden lets us locate the byte; a non-persisting write field would only mislead an AI.

### PR-9 — PcbLib: Region KIND / net / name (format)

**Primitive:** Region. **Kind:** read+write+tool. **Missing:** KIND (Copper vs Cutout — *the* defining region property), NET, NAME, CAVITYHEIGHT, plus
flags/holes/unique_id on write schema.
**Fix:** Parse the C-string param block in `parse_region` (currently skipped by length); add `kind`/`net`/`name`/`cavity_height` fields; emit real values in
`encode_region_properties` instead of hard-coded `KIND=0`; expose kind/net/name/flags/holes in schema. Today any non-copper region read from Altium becomes copper.

### PR-10 — PcbLib: Text mirror, TrueType font/bold, justification (format)

**Primitive:** Text. **Kind:** read+write+tool. **Missing:** `mirror` (@35 — bottom-side silk), `font_name` (UTF-16 @46), `bold` (@44), `justification` (dead field @132),
plus tool-side `kind`/`stroke_font`/`italic`/`flags`.
**Fix:** Add `mirror`/`font_name`/`bold` to struct; read/write the offsets; wire `justification` end-to-end (reader currently hard-codes default); stop hard-coding
`kind`/`stroke_font`/`italic`/`flags` in `parse_text`; add all to schema.

### PR-11 — PcbLib: ComponentBody layer bug + authoring fields

**Primitive:** ComponentBody. **Kind:** read+tool. **Missing:** **Layer reader bug** — `parse_v7_layer` maps only MECHANICAL2-7, collapsing everything else (incl.
golden's MECHANICAL13) to Top3DBody; plus body_color_3d/opacity/projection/model_2d_rotation/is_shape_based/kind/model linkage all hard-coded in the write handler.
**Fix:** Extend `parse_v7_layer` to all MECHANICAL1-32 (or read the layer byte); stop hard-coding the 10+ body fields in `read_write.rs:679-706` and add them to the
schema. Layer fix is a one-function bug; cavity_height/model_2d_location need new struct fields.

### PR-12 — SchLib: Pin authoring fields (description, swap, colour, types)

**Primitive:** Pin. **Kind:** tool. **Missing:** `description`, `colour`, `graphically_locked`, `swap_id_group`, `part_and_sequence`, `default_value` hard-coded in
`parse_schlib_pin`; symbol-edge decorations undocumented; electrical_type enum missing open_collector/open_emitter/hiz.
**Fix:** Read these keys in `parsing.rs` instead of hard-coding; add to pin schema; extend the advertised `electrical_type` enum to include
open_collector/open_emitter/hiz/tristate (parser already supports them). Description + open-collector/Hi-Z are ERC-meaningful.

### PR-13 — SchLib: line_style / transparent / fill on write for all shapes

**Primitives:** Rectangle, RoundRect, Ellipse, Polyline, Polygon, Line, SchArc. **Kind:** tool. **Missing:** `line_style` (dashed/dotted), `transparent`,
`fill_color`/`filled` hard-coded in the `parse_schlib_*` functions; Polyline/Polygon also missing AreaColor+IsSolid at struct level.
**Fix:** Read these keys per shape in `parsing.rs`; add to schemas. For Polyline/Polygon, add `fill_color`+`filled` to the struct/reader/writer (Polygon-specific omission
vs rect/ellipse). Also expose Polyline start/end_line_shape + line_shape_size (arrowheads) on write.

### PR-14 — SchLib: universal display/lock flags across all shapes (format)

**Primitives:** Rectangle, RoundRect, Ellipse, Line, Polyline, Polygon, SchArc, Label, Parameter. **Kind:** read+write. **Missing:** GraphicallyLocked, Disabled, Dimmed,
OwnerPartDisplayMode — none modelled on *any* SchLib shape.
**Fix:** Add these four fields to a shared shape base (or per struct); read/write the Altium keys in every `parse_*`/`encode_*`. OwnerPartDisplayMode (de-Morgan/alternate
views) and GraphicallyLocked are the EE-meaningful ones. Large but mechanical; can be split per-primitive if needed.

### PR-15 — SchLib: Parameter display properties (format)

**Primitive:** Parameter. **Kind:** read+write+tool. **Missing:** Justification, Orientation, ShowName/HideName, IsMirrored, Description, AreaColor, AutoPosition,
IsConfigurable/IsRule/IsSystemParameter, plus tool-side param_type/read_only_state (hard-coded).
**Fix:** Add the EE-meaningful fields (Orientation, ShowName/HideName, Description, IsConfigurable) to the Parameter struct + reader/writer; stop hard-coding
`param_type`/`read_only_state` in `parse_schlib_parameter` and add to the manage/create schemas.

### PR-16 — SchLib: %UTF8%Text parameter decoding (correctness)

**Primitive:** Parameter. **Kind:** read+write. **Missing:** when a value has non-Win-1252 chars, Altium uses the `%UTF8%Text` key; our reader only reads plain `text` as
Win-1252, *corrupting* non-Latin values.
**Fix:** In `parse_label`/parameter reader, check for `%UTF8%Text` and decode UTF-8; re-emit it on write when the value needs it. *This is data corruption, not just loss
— worth prioritising despite the "round-trip" tag.*

---

## 3. ROUND-TRIP / FIDELITY GAPS (lower priority)

### PR-R1 — unique_id preservation on write (all primitives)

**Primitives:** Pad, Via, Track, Arc, Region, Text (PcbLib); Rectangle, RoundRect, Ellipse, Line, Polyline, SchArc, Label, Parameter (SchLib). **Kind:** tool/read.
**Fix:** Stop hard-coding `unique_id: None` in every `parse_*`; accept it on write so read→write preserves identity. PcbLib Pad/Text/ComponentBody readers also never
*extract* the GUID — fix those to read the UniqueIDs/PrimitiveGuids streams. Breaks stable primitive identity across saves today.

### PR-R2 — Fractional sub-coordinates (*_FRAC) across SchLib

**Primitives:** Rectangle, RoundRect, Ellipse, Line, Polyline, Polygon, SchArc, Label, Parameter. **Kind:** read.
**Fix:** Mirror the existing EllipticalArc `_frac` handling: read LOCATION/CORNER/RADIUS `_FRAC` keys via `CoordFromDxp`; store coords as the combined value; emit `_FRAC`
on write. Off-grid primitives lose sub-unit precision today.

### PR-R3 — SchLib Pin auxiliary streams (PinFrac, PinSymbolLineWidth)

**Primitive:** Pin. **Kind:** read. **Missing:** symbol_line_width (decoration thickness — arguably feature-loss), fractional pin coords, owner_part_display_mode (skipped
byte).
**Fix:** Parse + write the `PinSymbolLineWidth` and `PinFrac` OLE streams; store `owner_part_display_mode` instead of `offset += 1`.

### PR-R4 — PcbLib net / component / polygon indices on read

**Primitives:** Pad, Via, Track, Arc, Region, Fill, Text, ComponentBody. **Kind:** read.
**Fix:** Read NetIndex/ComponentIndex/PolygonIndex from the common header (currently hard-coded 0xFF on write, skipped on read). Negligible for netless library footprints
but needed for board-context primitives and full fidelity.

### PR-R5 — PcbLib Region/ComponentBody param-block round-trip

**Primitives:** Region, ComponentBody. **Kind:** read.
**Fix:** Capture unmodelled param keys into an `additional_parameters` map (Region: SUBPOLYINDEX/UNIONINDEX/ARCRESOLUTION/ISSHAPEBASED; ComponentBody: texture set,
model_2d_location, model_type/source, identifier, both ARCRESOLUTIONs) and re-emit them instead of hard-coded literals. ComponentBody also: raw outline precision, flags,
model_2d_location (feature-loss-ish), cavity_height.

### PR-R6 — Per-primitive identity GUID extraction (PcbLib Via/Pad)

**Primitives:** Via, Pad. **Kind:** write. Currently every emitted via/pad shares one template GUID (IdentityGuid/IdentityGuidB). **Fix:** Generate/preserve unique
per-primitive GUIDs in `encode_via`/`encode_pad`.

### PR-R7 — IsNotAccessible faithful round-trip (SchLib)

**Primitives:** Rectangle, Polygon, Line, Label, Arc. **Kind:** write. Writer hard-codes `IsNotAccesible=T`. **Fix:** Read the real value into the struct and emit it
(accessible `=F` cannot round-trip today).

---

## 4. COSMETIC GAPS (lowest priority)

- **PcbLib Pad:** IsHidden/Enabled/UserRouted, SolderMaskExpansionFromHoleEdge(WithRule), test-point flags (IsTestPointTop/Bottom/Assy), JumperID, Simple-pad mid/bottom
    size divergence.
- **PcbLib Via:** test-point/backdrill/teardrop/counter-hole/pre-route flags, DrillLayerPairType, back-side mask Option-vs-always representation.
- **PcbLib Track/Arc:** NetIndex/ComponentIndex (always 0xFFFF in libraries).
- **PcbLib Text:** char_set, union_index, is_comment/is_designator bytes, inverted-rect text-box block, barcode block, frame/snap tail. *(Inverted-rect & barcode are
    larger features but niche — defer.)*
- **PcbLib Region:** RawFlags spare bits, OutlineExact sub-coord precision.
- **SchLib Pin:** hidden/show_name/show_designator (schema-doc only; parser already accepts).
- **SchLib Arc/Label/Parameter:** IndexInSheet, OwnerIndex (regenerated positionally; usually -1/implicit).
- **SchLib Parameter:** Designator-specific group (NameIsReadOnly, AllowDatabaseSynchronize, PhysicalDesignator, VariantOption), TextHorz/VertAnchor.

---

## 5. ALREADY-COMPLETE PRIMITIVES

**None are fully complete.** Closest:

- **PcbLib Track** and **PcbLib Arc** — geometry is fully read+written+tool-exposed; remaining gaps are *only* the write-tool flags/mask (PR-4) and netless round-trip
    fidelity. After PR-4 these two are effectively done.
- **SchLib Pin** — best-covered overall (core binary record fully round-trips); blocked only by tool-schema hard-coding (PR-12) and aux streams (PR-R3).

**Suggested execution order:** PR-1 → PR-2 → PR-3 → PR-4 (unblock authoring of all primitives) → PR-12, PR-13 (SchLib tool fields) → PR-6, PR-7, PR-8, PR-9, PR-10 (PcbLib
format EE features) → PR-11, PR-14, PR-15 → PR-16 (correctness) → round-trip PRs → cosmetic.

## Per-primitive gap inventory

Totals: feature-loss=129, round-trip=99, cosmetic=1 | by kind: read=156, tool=69, write=4

### PcbLib Pad (gaps: 21)

Audited PcbLib Pad coverage across our model (struct src/altium/pcblib/primitives/pads.rs, reader src/altium/pcblib/reader/parsers.rs::parse_pad, writer
src/altium/pcblib/writer.rs::encode_pad/build_pad_extended_tail/encode_pad_size_shape_block), and the MCP tools (write path src/mcp/tools/parsing.rs::parse_pad + schema
src/mcp/tool_definitions.rs; read path src/mcp/tools/read_write.rs serde-serializes the whole Pad struct) versus AltiumSharp ground truth (C:/tmp/AltiumSharp PcbPad.cs +
PcbLibReader.cs::ReadPad/ReadPadExtendedFields/ReadPadSizeShapeBlock). Our model fully round-trips the core SMD/through-hole geometry: designator, x/y, top/mid/bottom
sizes, hole size, top/mid/bottom shapes, rotation, plated (derived), hole shape, paste/solder mask expansion + tri-state modes, corner radius, stack mode, per-layer
sizes/shapes/corner-radii/offsets (FullStack), and the lock/tenting/keepout flags. Thermal-relief, power-plane, net, hole tolerances/slot/rotation, drill type,
swap/jumper IDs, testpoint and several layer/render flags are NOT modelled. The biggest AI-facing gaps: thermal-relief (PowerPlaneConnectStyle, ReliefConductorWidth,
ReliefEntries, ReliefAirGap), power-plane clearance/expansion, net assignment, hole tolerances, jumper/testpoint flags. Many of the modelled fields are also not exposed
by the write tool schema (no per-layer/stack, no flags/tenting, no unique_id), and unique_id is dropped on read and regenerated on write.

- **[feature-loss | read]** `PowerPlaneConnectStyle (thermal-relief connect style: 0=Relief, 1=Direct, 2=NoConnect)` — AltiumSharp reads this from SubRecord-5 offset 67
    (ReadPadExtendedFields B(67)). Our Pad struct has no field for it; parse_pad never reads offset 67. EE-meaningful: controls how a through-hole pad connects to a
    power/ground plane. An AI cannot read or set whether a pad is direct-connect vs thermal-relief vs no-connect. The Via struct models thermal relief but Pad does not.
    Lost on read, not in struct, not writable, not tool-exposed.
- **[feature-loss | read]** `ReliefConductorWidth (thermal relief spoke width)` — AltiumSharp reads I32 at offset 68. No corresponding Pad struct field; writer leaves the
    template default (offsets 68-71 in PAD_EXTENDED_TAIL_TEMPLATE are fixed bytes). An AI placing a TH pad on a plane cannot control the thermal spoke width. Lost on
    read, hard-coded on write, not tool-exposed.
- **[feature-loss | read]** `ReliefEntries (number of thermal relief spokes/conductors)` — AltiumSharp reads I16 at offset 72 (default 4). No Pad struct field; writer
    emits the template value only. An AI cannot set the spoke count (2 vs 4) for a pad's plane connection. Lost on read, fixed on write, not tool-exposed.
- **[feature-loss | read]** `ReliefAirGap (thermal relief air gap)` — AltiumSharp reads I32 at offset 74. No Pad struct field; writer keeps the template default.
    EE-meaningful thermal-relief parameter; an AI cannot read or set it for a pad. Lost on read, fixed on write, not tool-exposed.
- **[feature-loss | read]** `PowerPlaneClearance (anti-pad clearance to plane)` — AltiumSharp reads I32 at offset 82. No Pad struct field; not parsed; writer keeps
    template bytes. Controls the clearance ring where a TH pad passes through a plane it is NOT connected to. Lost on read, fixed on write, not tool-exposed.
- **[feature-loss | read]** `PowerPlaneReliefExpansion` — AltiumSharp reads I32 at offset 78. No Pad struct field; not parsed; template-only on write. Part of the
    plane-connection geometry. Lost on read, fixed on write, not tool-exposed.
- **[feature-loss | read]** `Net / NetIndex (net assignment)` — AltiumSharp PcbPad exposes Net (string) and NetIndex (ushort, default 0xFFFF), read from common-primitive
    data and the SubRecord-3 net string (PadNetString). Our reader reads SubRecord-3 (Block 2) but discards it ('Block 2: Unknown string (|&|0)') and writer hard-codes
    '|&|0'. Our Pad has no net field. For a PcbLib footprint nets are usually empty, but the field is a real pad property an AI may want to read/echo. Lost on read, fixed
    on write, not tool-exposed.
- **[round-trip | read]** `HoleType / hole_shape on read path` — Our reader DOES read hole shape (parsers.rs reads size/shape block offset 262 -> HoleShape) and the
    writer emits it, so Round/Square/Slot round-trips. NOT a feature gap. Listed only to note that a Slot hole's geometry (HoleSlotLength, HoleRotation) is NOT captured —
    see those entries.
- **[feature-loss | read]** `HoleSlotLength (slot hole length)` — AltiumSharp ReadPadSizeShapeBlock reads holeSlotLength (i32 in the 596-byte block) and
    PcbPad.HoleSlotLength. Our reader reads the hole TYPE byte at offset 262 but never reads the slot length; the writer hard-codes 'write_i32(0) // hole slot length (not
    modelled)' (writer.rs:482). So a Slot hole has no length — an AI can request hole_shape='slot' but cannot specify the slot length, producing a degenerate/zero-length
    slot. Lost on read, written as 0, not in struct, not tool-exposed.
- **[feature-loss | read]** `HoleRotation (rotation of a slot/non-round hole)` — AltiumSharp reads holeRotation (f64) in the size/shape block and PcbPad.HoleRotation. Our
    reader never reads it; writer hard-codes 'write_f64(0.0) // hole rotation' (writer.rs:483). A rotated slot hole cannot be represented. Lost on read, written as 0, not
    in struct, not tool-exposed.
- **[feature-loss | read]** `HolePositiveTolerance / HoleNegativeTolerance (drill tolerances)` — AltiumSharp reads I32 at offsets 162/166 (default 0x7FFFFFFF = unset)
    into PcbPad.HolePositiveTolerance/HoleNegativeTolerance. Our reader does not read these; our extended-tail template has the 0x7FFFFFFF sentinel hard-coded at 162-169
    (writer.rs:543 bytes FF FF FF 7F FF FF FF 7F), so we always write 'unset'. A pad authored with a real drill +/- tolerance cannot be read back or written. Lost on
    read, fixed sentinel on write, not in struct, not tool-exposed.
- **[feature-loss | read]** `JumperID` — AltiumSharp reads I16 at offset 110 into PcbPad.JumperID; PcbPadDto also exposes JUMPERID. Used to group pads as a jumper/0-ohm
    link and for test-point identification. Our reader never reads it; writer keeps template bytes; no struct field. Lost on read, fixed on write, not tool-exposed.
- **[feature-loss | read]** `DrillType (0=Simple, 1=Pressfit)` — AltiumSharp PcbPad.DrillType distinguishes a plated drilled hole from a press-fit hole. Our model has no
    field and the reader does not parse it. EE-meaningful for connectors. Lost on read, not in struct, not writable, not tool-exposed.
- **[feature-loss | read]** `IsTestPointTop / IsTestPointBottom / IsAssyTestpointTop / IsAssyTestpointBottom` — AltiumSharp decodes these test-point flags for the pad.
    Our PcbFlags only models LOCKED/POLYGON/KEEPOUT/TENTING_TOP/TENTING_BOTTOM, so the fabrication/assembly test-point flags are dropped on read and cannot be written or
    set via the tool. Lost on read, not modelled, not tool-exposed.
- **[feature-loss | tool]** `PcbFlags tenting/keepout/locked NOT exposed by write tool schema` — The Pad struct has a `flags` field (LOCKED, KEEPOUT, TENTING_TOP,
    TENTING_BOTTOM) that the reader populates and the read JSON serializes, and the writer encodes. But the write tool (mcp/tools/parsing.rs::parse_pad) hard-codes flags:
    PcbFlags::empty() and the write_pcblib schema (tool_definitions.rs pad items: only designator/x/y/width/height/shape/layer/hole_size) does not accept any
    flag/tenting/lock/keepout key. An AI can READ a pad's tenting/lock state but cannot SET it when authoring a footprint.
- **[feature-loss | tool]** `stack_mode / per_layer_sizes / per_layer_shapes / per_layer_corner_radii / per_layer_offsets NOT exposed by write tool schema` — These fields
    exist on the struct, are read by parse_pad (parsers.rs), serialized in the read JSON, and encoded by the writer (TopMiddleBottom in main block, FullStack via
    per-layer block). But the write tool's parse_pad (parsing.rs) hard-codes stack_mode: PadStackMode::Simple and all per_layer_* : None, and the write_pcblib schema has
    no stack_mode/per_layer keys. An AI cannot author a per-layer (top/mid/bottom or full-stack) pad through the tool, even though the model supports it.
- **[feature-loss | tool]** `hole_shape / paste_mask_expansion / solder_mask_expansion / *_mode / corner_radius_percent / rotation missing from write_pcblib JSON schema`
    — mcp/tools/parsing.rs::parse_pad DOES read these keys from JSON (rotation, hole_shape, paste_mask_expansion, solder_mask_expansion, paste_mask_expansion_mode,
    solder_mask_expansion_mode, corner_radius_percent), so they are accepted on write. But the write_pcblib input schema in tool_definitions.rs (pad items.properties)
    only documents designator/x/y/width/height/shape/layer/hole_size. An AI relying on the advertised schema will not know it can set rotation, hole shape, mask
    expansions/modes, or corner radius — they are undocumented and effectively undiscoverable.
- **[round-trip | read]** `unique_id (pad UNIQUEID) dropped on read, regenerated on write` — The Pad struct has unique_id (serialized in read JSON when Some), and
    PcbPadDto exposes UNIQUEID; AltiumSharp models IdentityGuid (GUID-A, per-pad) and IdentityGuidB (GUID-B, footprint/stack). Our reader hard-codes unique_id: None
    (parsers.rs:257) — it never extracts the GUID at extended-tail offset 126. The writer ignores any provided unique_id and always generates two fresh GUIDs
    (writer.rs:598-599 generate_guid). So a read-modify-write does not preserve the pad's identity GUID; a previously-read unique_id cannot be round-tripped. Lost on
    read; not honoured on write.
- **[feature-loss | read]** `SolderMaskExpansionFromHoleEdge / SolderMaskExpansionFromHoleEdgeWithRule` — AltiumSharp PcbPad exposes these booleans controlling whether
    solder-mask expansion is measured from the hole edge rather than the pad edge. Not modelled by our Pad and not parsed. Affects mask geometry for through-hole pads.
    Lost on read, not in struct, not writable, not tool-exposed.
- **[round-trip | read]** `IsHidden / Enabled / IsKeepout(non-flag) / UserRouted` — AltiumSharp models render/state booleans (IsHidden, Enabled default true, UserRouted;
    PcbPadDto exposes USERROUTED). Our model captures KEEPOUT via PcbFlags but not Hidden/Enabled/UserRouted. These are mostly board-render state rather than
    footprint-authoring fields, so lower EE value for a library pad, but they are real pad properties dropped on read and not preserved on round-trip.
- **[round-trip | write]** `SizeMiddle/SizeBottom & ShapeMiddle/ShapeBottom for Simple pads (round-trip)` — AltiumSharp keeps distinct SizeMiddle/SizeBottom and
    ShapeMiddle/ShapeBottom even for a Simple pad (read from main-block offsets 29-44 / 50-51). For a Simple (non-TopMiddleBottom) pad our reader only keeps the top
    width/height/shape and discards mid/bottom; the writer re-emits top size/shape into all three slots. Altium normally keeps these equal for a simple pad so this is
    usually benign, but a hand-authored pad whose stored mid/bottom differ while stackMode=0 would not round-trip. Mid/bottom only surface when
    stack_mode==TopMiddleBottom.

### Via (PcbLib) (gaps: 17)

Audited the PcbLib Via across our Rust model (struct src/altium/pcblib/primitives/pads.rs Via, reader src/altium/pcblib/reader/parsers.rs parse_via, writer
src/altium/pcblib/writer.rs encode_via), the MCP tools (read JSON src/mcp/tools/read_write.rs line 334 `"vias": fp.vias`; write schema src/mcp/tool_definitions.rs and
write handler call_write_pcblib), versus AltiumSharp ground truth (C:/tmp/AltiumSharp Models/Pcb/PcbVia.cs + Serialization/Readers/PcbLibReader.cs ReadVia, plus the
parameter DTO PcbViaDto.cs). The golden footprints.PcbLib contains no via so AltiumSharp's binary ReadVia (SubRecord-1 offset map) is the authoritative field list. Our 15
struct fields (x, y, diameter, hole_size, from_layer, to_layer, solder_mask_expansion, solder_mask_expansion_mode, solder_mask_expansion_back, thermal_relief_gap,
thermal_relief_conductors, thermal_relief_width, diameter_stack_mode, per_layer_diameters, unique_id) are all read (unique_id via the separate
UniqueIDPrimitiveInformation stream in apply_unique_ids), written, and surfaced on the read JSON — so for what we model, read coverage is solid. The dominant gap is
asymmetric tooling: the Via is entirely absent from the WRITE path. tool_definitions.rs write_pcblib footprint schema lists
pads/tracks/arcs/regions/text/step_model/component_bodies but has NO `vias` property, and call_write_pcblib (read_write.rs) parses each of those arrays explicitly but
never reads `fp_json.get("vias")`. So via geometry is readable but an AI cannot author a via through the documented write_pcblib tool at all (only the undocumented
import_components serde path would accept it). Beyond that, AltiumSharp stores ~20 Via properties our struct never models (net/component index, tenting top/bottom,
keepout, locked, power-plane connect style/clearance/relief, paste-mask expansion, back-side solder-mask as a first-class field, drill tolerances, drill-layer-pair type,
solder-mask-from-hole-edge, test-point/backdrill/teardrop/user-routed flags, per-via identity GUIDs). Most are EE-meaningful (tenting, net, power-plane connection, paste
mask) and are silently dropped on read and forced to template defaults on write.

- **[feature-loss | tool]** `vias (write_pcblib input schema)` — src/mcp/tool_definitions.rs write_pcblib footprint 'items.properties' lists
    pads/tracks/arcs/regions/text/step_model/component_bodies but has NO 'vias' property. The Via primitive is not documented as writable, so an AI driving write_pcblib
    has no schema telling it vias can be placed or what fields they take.
- **[feature-loss | tool]** `vias (write_pcblib handler parsing)` — src/mcp/tools/read_write.rs call_write_pcblib explicitly parses
    fp_json.get("pads"/"tracks"/"arcs"/"regions"/"text"/"step_model"/"component_bodies") but never calls fp_json.get("vias"). Even if an AI sends a 'vias' array it is
    silently ignored on the primary write tool — no via is ever added to the Footprint. (Only the undocumented import_components serde-from_value path would accept it.)
    Net effect: read exposes vias but the AI cannot create or modify a via through write_pcblib.
- **[feature-loss | read]** `Net / NetIndex (SubRecord-1 offset 3-4; param NET)` — AltiumSharp reads via.NetIndex = U16(3) and PcbViaDto exposes a NET string. Our Via
    struct has no net field; parse_via starts at offset 13 and never reads offsets 3-4, so net membership is dropped on read. encode_via hardcodes
    write_common_header(..., PcbFlags::empty()) which fills bytes 3-12 with 0xFF (no net), so a via can never be assigned to a net on write either.
- **[feature-loss | read]** `IsTentingTop / IsTentingBottom (flags word, SubRecord-1 bytes 1-2)` — AltiumSharp decodes the flags word at bytes 1-2 into
    IsTentingTop/IsTentingBottom (whether each face is covered by solder mask). Our Via struct has no tenting field and parse_via never reads bytes 1-2. encode_via passes
    PcbFlags::empty(), so tenting is always written as off. Tenting is a primary EE-meaningful via property an AI would want to set; it is dropped on read and unsettable
    on write. (Note: our model approximates tenting only indirectly via solder_mask_expansion_back, not the actual tenting flags.)
- **[feature-loss | read]** `IsKeepout (flags word, SubRecord-1 bytes 1-2)` — AltiumSharp decodes IsKeepout from the flags word. Our Via has no keepout field and
    parse_via ignores the flags bytes; encode_via writes PcbFlags::empty(). A keepout via is read back as a normal via and cannot be authored as keepout.
- **[feature-loss | read]** `IsLocked (flags word, SubRecord-1 bytes 1-2)` — AltiumSharp decodes IsLocked from the flags word. Our Via has no locked field; parse_via
    ignores the flags bytes and encode_via writes the unlocked default. Lock state is lost on read and always written unlocked.
- **[feature-loss | read]** `PowerPlaneConnectStyle (SubRecord-1 offset 31)` — AltiumSharp reads via.PowerPlaneConnectStyle = B(31) (0=Relief,1=Direct,2=NoConnect). Our
    Via has no such field, parse_via never reads offset 31, and encode_via leaves the template byte untouched (cannot be set). This controls how the via connects to
    internal power planes — EE-meaningful.
- **[feature-loss | read]** `PowerPlaneReliefExpansion (SubRecord-1 offset 42-45)` — AltiumSharp reads via.PowerPlaneReliefExpansion = Coord.FromRaw(I32(42)). Not
    modelled; parse_via skips offset 42, encode_via keeps the template value (unsettable). Power-plane thermal relief geometry is lost on read.
- **[feature-loss | read]** `PowerPlaneClearance (SubRecord-1 offset 46-49)` — AltiumSharp reads via.PowerPlaneClearance = Coord.FromRaw(I32(46)). Not modelled; parse_via
    skips offset 46 and encode_via keeps the template value. Plane antipad clearance is lost on read and unsettable.
- **[feature-loss | read]** `PasteMaskExpansion (SubRecord-1 offset 50-53)` — AltiumSharp reads via.PasteMaskExpansion = Coord.FromRaw(I32(50)). Our Via only models
    solder-mask expansion, not paste-mask. parse_via skips offset 50; encode_via keeps the template value. Paste-mask expansion is lost on read and cannot be set.
- **[feature-loss | read]** `SolderMaskExpansionFromHoleEdge (SubRecord-1 offset 258)` — AltiumSharp reads via.SolderMaskExpansionFromHoleEdge = B(258) != 0 (whether mask
    expansion is measured from the hole edge vs the pad edge). Not modelled; parse_via never reads offset 258 and encode_via keeps the template default. Lost on read,
    unsettable.
- **[feature-loss | read]** `DrillLayerPairType (SubRecord-1 offset 312)` — AltiumSharp reads via.DrillLayerPairType = B(312) (0=Through,1=BlindBuriedStart,2=Mid,3=End).
    Our model infers span only from from_layer/to_layer and has no explicit drill-pair-type. parse_via skips offset 312, encode_via keeps the template value. The
    blind/buried classification byte is lost on read and unsettable.
- **[feature-loss | read]** `HolePositiveTolerance / HoleNegativeTolerance (SubRecord-1 offsets 291-294, 295-298)` — AltiumSharp reads HolePositiveTolerance = I32(291)
    and HoleNegativeTolerance = I32(295) (default sentinel 0x7FFFFFFF = unset). Not modelled; parse_via skips them and encode_via leaves the template bytes (which happen
    to carry the 0x7FFFFFFF sentinel). Manufacturing drill tolerances are lost on read and cannot be authored.
- **[feature-loss | read]** `Test-point / fabrication flags (IsTestpointTop/Bottom, IsAssyTestpointTop/Bottom, IsBackdrill, TearDrop, UserRouted, IsCounterHole,
    IsPreRoute)` — PcbVia.cs and PcbViaDto (USERROUTED, etc.) carry numerous fabrication/test flags. Our Via models none of them; parse_via reads no flag bits for these
    and encode_via cannot set them. IsTestpoint in particular is EE-meaningful (a via used as a bed-of-nails test point). All lost on read, unsettable on write.
- **[round-trip | read]** `Back-side solder-mask raw (SubRecord-1 offset 242-245)` — AltiumSharp always reads via.SolderMaskBackRaw = I32(242) as a first-class value. Our
    reader only surfaces solder_mask_expansion_back when offset-242 differs from the front offset-54 (match arm in parse_via returns None when equal). Functionally
    round-trips for the common equal case, but a deliberately-zero back-mask that equals a zero front is collapsed to None, and the field is presented as an Option rather
    than the always-present value Altium stores — minor fidelity/representation gap, not data-corrupting.
- **[round-trip | write]** `IdentityGuid (uid, offsets 259-274) / IdentityGuidB (sig, offsets 275-290)` — AltiumSharp round-trips two per-via identity GUIDs from
    SubRecord-1 (uid at 259, sig at 275). Our reader never extracts them and encode_via reuses the fixed VIA_SR1_TEMPLATE GUID bytes for every via, so multiple vias
    written by us share one identity GUID rather than preserving/generating unique ones. Not AI-facing and not corrupting (Altium revalidates), but a byte-fidelity gap on
    read-modify-write of an existing via and a uniqueness gap when emitting several vias.
- **[round-trip | read]** `ComponentIndex (SubRecord-1 offset 7-8)` — AltiumSharp reads via.ComponentIndex = U16(7) (-1 when 0xFFFF). Not modelled; parse_via skips
    offsets 7-8 and encode_via writes 0xFFFF (free primitive) via the common header. In a footprint/library context vias are component-free so this is usually -1, but the
    stored linkage is dropped on read and forced free on write.

### Track (PcbLib) (gaps: 6)

Audited PcbLib Track coverage across our Rust (struct src/altium/pcblib/primitives/shapes.rs, reader src/altium/pcblib/reader/parsers.rs::parse_track + mod.rs
apply_unique_ids/read_flags, writer src/altium/pcblib/writer.rs::encode_track) vs AltiumSharp ground truth (Models/Pcb/PcbTrack.cs + the authoritative binary
Serialization/Readers/PcbLibReader.cs::ReadTrack), sanity-checked against the golden scripts/samples/footprints.PcbLib (49-byte TRACK records). The binary ReadTrack reads
exactly: layer@0, flags@1-2 (decoded to IsLocked/IsTentingTop/IsTentingBottom/IsKeepout), NetIndex@3-4, ComponentIndex@7-8, Start/End XY@13/17/21/25, Width@29,
SolderMaskExpansion@35-38, KeepoutRestrictions@45 (the many PowerPlane/ReliefAirGap/PasteMask/UserRouted/TearDrop/testpoint props on the PcbTrack model are NOT populated
by the binary PcbLib reader — they belong to the text PcbDoc DTO/board context — so they are out of scope for PcbLib footprints). Geometry (x1/y1/x2/y2/width/layer) is
fully read+written+tool-exposed = no gap. Our struct fully covers flags, unique_id, solder_mask_expansion, keepout_restrictions, and the reader+writer round-trip all of
them, AND the read tool serialises the whole struct (so all are READ-exposed). The dominant gap is on the WRITE TOOL: tool_definitions.rs write_pcblib track schema only
declares x1/y1/x2/y2/width/layer, and parsing.rs::parse_track calls Track::new() and never reads flags/unique_id/solder_mask_expansion/keepout_restrictions — so an AI
authoring a track via write_pcblib cannot set or preserve any of them. NetIndex/ComponentIndex are not modelled at all (writer hardcodes 0xFF, reader skips), but these
are 0xFFFF/none for footprint-library tracks (confirmed in golden) so they are round-trip fidelity only, not EE-meaningful for a library.

- **[feature-loss | tool]** `flags (IsLocked / IsKeepout / IsTentingTop / IsTentingBottom)` — Struct has `flags: PcbFlags`, the reader decodes the Altium flag word
    (read_flags) and the writer re-encodes it, and the read tool serialises it — but the write tool drops it. tool_definitions.rs write_pcblib `tracks` item schema (lines
    209-218) declares only x1/y1/x2/y2/width/layer, and parsing.rs::parse_track (line 212) calls `Track::new()` which forces flags=empty. So an AI cannot author a locked
    or keepout track, nor preserve those bits when round-tripping through write_pcblib. (Contrast: parse_pad/other parsers DO read flags.)
- **[feature-loss | tool]** `solder_mask_expansion` — Real EE property (mask opening over the track). Fully modelled: struct field, parsed at byte offset 35-38
    (parsers.rs:533), written at 35-38 (writer.rs:825), and emitted in read JSON. But the write tool omits it: not in the tool_definitions track schema and parse_track
    never reads a `solder_mask_expansion` key, so an AI reading a non-zero value cannot write it back or set a new one — it is silently reset to 0 on write_pcblib.
- **[feature-loss | tool]** `keepout_restrictions` — Per-object keepout restriction bitmask (Altium KeepoutRestrictions byte @45). Modelled in struct, parsed
    (parsers.rs:534), written (writer.rs:829, byte 45), and present in read JSON, but absent from the write_pcblib track schema and from parse_track. An AI cannot set or
    preserve it through the write tool; it is forced to 0 on write.
- **[round-trip | tool]** `unique_id (UNIQUEID)` — Altium per-primitive UniqueId. Read end is complete (apply_unique_ids in reader/mod.rs:287 attaches it to tracks from
    the UniqueIDs stream; writer.rs:1603 re-emits it; read JSON exposes it). But the write tool drops it: not in the tool_definitions track schema and parse_track calls
    Track::new() leaving unique_id=None. An AI editing-then-rewriting a footprint via write_pcblib loses each track's existing UniqueId (a fresh one is assigned),
    breaking stable primitive identity across saves.
- **[round-trip | read]** `NetIndex (bytes 3-4)` — Altium stores a 16-bit net index at offset 3-4 (AltiumSharp ReadTrack: track.NetIndex = U16(3)). Our model has no field
    for it: the reader never reads bytes 3-12 (parse_track jumps straight to offset 13) and the writer hardcodes bytes 3-12 to 0xFF (write_common_header, writer.rs:258).
    For a footprint LIBRARY this is always 0xFFFF/none (confirmed in golden footprints.PcbLib: net=0xffff), so EE value is negligible — flagged only as byte/round-trip
    fidelity if a netted board track were ever read.
- **[round-trip | read]** `ComponentIndex (bytes 7-8)` — Altium stores a 16-bit component index at offset 7-8 (AltiumSharp ReadTrack: ComponentIndex = U16(7), 0xFFFF =
    none). Our reader never reads it and the writer hardcodes it to 0xFF via the common-header 0xFF fill. In a footprint library it is 0xFFFF (confirmed in golden), so
    this is round-trip/byte fidelity only, not an AI-facing feature for library work.

### Arc (PcbLib) (gaps: 5)

Audited the PcbLib Arc primitive across our model (struct src/altium/pcblib/primitives/shapes.rs Arc; reader src/altium/pcblib/reader/parsers.rs::parse_arc; writer
src/altium/pcblib/writer.rs::encode_arc; MCP read src/mcp/tools/read_write.rs which serialises fp.arcs via serde, MCP write schema src/mcp/tool_definitions.rs +
src/mcp/tools/parsing.rs::parse_arc) vs Altium ground truth (C:/tmp/AltiumSharp ReadArc/WriteArc in PcbLibReader.cs:937 / PcbLibWriter.cs:643, model
Models/Pcb/PcbArc.cs). Sanity-checked against the golden ARCS/Data stream in scripts/samples/footprints.PcbLib (60-byte arc block: layer@0, flags@1-2=0x000c,
net@3-4=0xFFFF, comp@7-8=0xFFFF, x/y@13-20, r@21, angles@25-40, width@41, smExp@47-50, keepout@56). AltiumSharp's BINARY ReadArc/WriteArc touch exactly: layer;
flags->IsLocked/IsTentingTop/IsTentingBottom/IsKeepout; NetIndex@3; ComponentIndex@7; centre x/y; radius; start/end angle; width; SolderMaskExpansion@47;
KeepoutRestrictions@56. (The many other PcbArc model bools/thermal/power-plane props are never serialised into the binary arc record, so they are NOT format gaps.) Our
geometry coverage is complete: x, y, radius, start_angle, end_angle, width, layer are read, written AND tool-exposed both ways. flags, solder_mask_expansion and
keepout_restrictions are read, written and visible on READ (struct is serialised directly), but the write tool neither documents nor accepts them — parse_arc hardcodes
flags=empty, solder_mask_expansion=None, keepout_restrictions=None. NetIndex/ComponentIndex are not modelled at all (writer hardcodes 0xFFFF, which matches a free
footprint primitive). No reader field is dropped for geometry; no writer field is lost for what the struct holds.

- **[feature-loss | tool]** `flags (IsLocked / IsTentingTop / IsTentingBottom / IsKeepout)` — Arc.flags (PcbFlags: LOCKED, KEEPOUT, TENTING_TOP, TENTING_BOTTOM) is read
    by parse_arc (read_flags) and written by encode_arc, and the read_pcblib JSON exposes it (fp.arcs serialised directly). But the write_pcblib input schema
    (tool_definitions.rs arcs.properties lists only x/y/radius/start_angle/end_angle/width/layer) does not document it, and parsing.rs::parse_arc hardcodes
    flags=PcbFlags::empty(), discarding any 'flags' the AI supplies. An AI therefore cannot create a locked or keepout arc, nor a tented one. Matches AltiumSharp
    ReadArc/WriteArc which encode these via EncodeFlags/DecodeFlags.
- **[feature-loss | tool]** `solder_mask_expansion` — Arc.solder_mask_expansion (Altium SolderMaskExpansion at block offset 47-50) is read and written for round-trip and
    appears in read_pcblib output, but the write_pcblib schema omits it and parse_arc hardcodes solder_mask_expansion=None. An AI cannot set per-arc solder-mask expansion
    on write (e.g. for an exposed/tented copper arc), even though Altium stores it and we already round-trip Altium-authored values.
- **[round-trip | tool]** `keepout_restrictions` — Arc.keepout_restrictions (Altium KeepoutRestrictions byte at offset 56, per-object-type keepout bitmask) is read,
    written and shown on read, but the write_pcblib schema omits it and parse_arc hardcodes keepout_restrictions=None. Niche bitmask, rarely AI-set, so round-trip rather
    than feature-loss; still unsettable via the write tool.
- **[round-trip | read]** `NetIndex` — AltiumSharp ReadArc reads NetIndex as U16 at offset 3-4 (0xFFFF = no net) and WriteArc emits it via WriteCommonPrimitiveData. Our
    Arc struct has no net field; parse_arc never reads bytes 3-4 and the writer's write_common_header hardcodes 0xFF for net. For a free footprint-library arc this is
    always 0xFFFF (matches the golden), so the data is not actually lost in practice, but a net-assigned arc copied from a board context would have its net dropped on
    read. Low EE value in a .PcbLib (footprints are netless).
- **[round-trip | read]** `ComponentIndex` — AltiumSharp ReadArc reads ComponentIndex as U16 at offset 7-8 (0xFFFF/-1 = not part of a component) and WriteArc emits it.
    Our Arc struct has no component field; parse_arc skips bytes 7-8 and write_common_header hardcodes 0xFF. Always 0xFFFF for free footprint primitives (matches golden),
    so effectively no real-world loss for a footprint library; flagged for completeness vs the authoritative field list.

### Region (PcbLib) (gaps: 15)

Our Region model is a thin geometry-only struct (vertices, holes, layer, flags{LOCKED/POLYGON/KEEPOUT/TENTING_TOP/TENTING_BOTTOM}, unique_id). AltiumSharp's PcbRegion
plus its ReadRegion parse the nested C-string parameter block into a rich set of typed EE-meaningful fields (KIND, NET, NAME, V7_LAYER, ARCRESOLUTION, CAVITYHEIGHT,
SUBPOLYINDEX, UNIONINDEX, ISSHAPEBASED, plus board-region/keepout-restriction/layer-stack keys) and reads the common-header NetIndex/ComponentIndex/PolygonIndex. Verified
against the committed golden REGION_COPPER.PcbLib, whose region Data stream contains "V7_LAYER=TOP|NAME=
|KIND=0|SUBPOLYINDEX=-1|UNIONINDEX=0|ARCRESOLUTION=0.5mil|ISSHAPEBASED=FALSE|CAVITYHEIGHT=0mil". Our reader (src/altium/pcblib/reader/parsers.rs:913 parse_region) skips
that whole param block by length (vertex_offset = 22 + param_len) and never parses a single key, so every param-derived field is lost on read; the writer
(src/altium/pcblib/writer.rs:1006 encode_region_properties) emits a fixed canonical param string with hardcoded defaults (KIND=0, NAME empty, etc.) and hardcodes the
common-header net/poly/component bytes to 0xFF, so even fields that mattered cannot round-trip; and the MCP write schema (src/mcp/tool_definitions.rs:237) accepts only
vertices+layer while the read JSON only exposes the five struct fields. Highest-value gap: KIND (a region's Copper vs Cutout vs board-cutout nature) is completely
invisible and unsettable. NOTE: layer is fully covered; flags-derived locked/keepout/tenting and holes/vertices are read+written; unique_id is read (UniqueIDs stream) and
written but not accepted on write nor exposed in the write schema. Shape-based regions (arc-capable PcbShapeBasedRegion, ShapeBasedRegions6 storage) are an entirely
separate primitive our MCP does not model at all (out of scope for this Region audit but noted).

- **[feature-loss | read]** `Kind (KIND param)` — AltiumSharp parses KIND from the region param block into PcbRegion.Kind (0=Copper, 1=Cutout, etc.) - the single most
    EE-meaningful region property, distinguishing a copper pour from a cutout/cavity. Our struct has no kind field and parse_region (parsers.rs:913) skips the param
    block, so it is lost on read. The writer hardcodes KIND=0, so any non-copper region read from Altium becomes copper. AI cannot read or set it.
- **[feature-loss | read]** `Net / NetIndex` — AltiumSharp reads the common-header NetIndex (parsers byte @3-4) and the NET param into PcbRegion.NetIndex/Net. Our
    parse_region reads only layer+flags from the header and skips the param block, dropping net assignment entirely. The writer hardcodes header bytes 3-12 to 0xFF (no
    net). A net-assigned copper region's net is invisible and unsettable - significant for copper pours.
- **[feature-loss | read]** `Name (NAME param)` — AltiumSharp parses NAME into PcbRegion.Name. Our struct has no name field; parse_region skips the param block so the
    region's name is lost on read, and the writer emits a fixed empty NAME=. Not exposed/settable by the AI.
- **[round-trip | read]** `ComponentIndex` — AltiumSharp reads the common-header ComponentIndex into PcbRegion.ComponentIndex (-1 = free). Our reader does not capture it
    and the writer hardcodes header bytes to 0xFF. For component-embedded regions this association is lost; mostly byte/association fidelity since footprint regions are
    typically free.
- **[round-trip | read]** `PolygonIndex` — AltiumSharp reads the common-header PolygonIndex into PcbRegion.PolygonIndex (which polygon a region belongs to). Our reader
    skips it and the writer hardcodes 0xFF. Lost on read; relevant for poured-polygon regions.
- **[round-trip | read]** `ArcApproximation (ARCRESOLUTION param)` — AltiumSharp parses ARCRESOLUTION (e.g. 0.5mil in the golden) into PcbRegion.ArcApproximation. Our
    reader skips the param block and the writer hardcodes ARCRESOLUTION=0mil, so the real arc-approximation tolerance does not round-trip (golden value 0.5mil becomes
    0mil).
- **[feature-loss | read]** `CavityHeight (CAVITYHEIGHT param)` — AltiumSharp parses CAVITYHEIGHT into PcbRegion.CavityHeight (embedded-component cavity depth). Our
    reader skips it and the writer hardcodes CAVITYHEIGHT=0mil. Lost on read; a real (if niche) EE property for cavity regions that the AI cannot read or set.
- **[round-trip | read]** `IsShapeBased (ISSHAPEBASED param)` — AltiumSharp parses ISSHAPEBASED into PcbRegion.IsShapeBased. Our reader skips it and the writer always
    emits ISSHAPEBASED=FALSE; a shape-based region read from Altium would silently lose the flag. Round-trip/format fidelity.
- **[round-trip | read]** `SubPolyIndex (SUBPOLYINDEX param)` — AltiumSharp parses SUBPOLYINDEX into PcbRegion.SubPolyIndex (-1 = not a sub-shape). Our reader skips it;
    writer hardcodes SUBPOLYINDEX=-1, so a region that was a polygon sub-shape loses its real index. Round-trip fidelity.
- **[round-trip | read]** `UnionIndex (UNIONINDEX param)` — AltiumSharp parses UNIONINDEX into PcbRegion.UnionIndex (groups unioned primitives). Our reader skips it;
    writer hardcodes UNIONINDEX=0, so a real grouping index does not round-trip. Round-trip fidelity.
- **[round-trip | read]** `AdditionalParameters (catch-all param round-trip)` — AltiumSharp captures every non-modeled param-block key into PcbRegion.AdditionalParameters
    and emits typed board-region keys (LAYER/KEEPOUT/ISBOARDCUTOUT/KEEPOUTRESTRICTIONS/PADINDEX/OBJECTKIND/BENDINGLINECOUNT/LOCKED3D/LAYERSTACKID) so unknown/optional
    keys round-trip. Our reader keeps no param map at all, so any such key on a real region is dropped and our writer cannot reproduce them. Byte/round-trip fidelity for
    non-corpus and board/rigid-flex regions.
- **[round-trip | read]** `OutlineExact / vertex sub-coordinate precision` — AltiumSharp keeps the raw IEEE-double outline (OutlineExact/HolesExact) so fractional
    sub-coordinate vertices round-trip byte-for-byte. Our reader quantises each vertex via x.round() as i32 then to_mm (parsers.rs:905), discarding sub-coord fractional
    bits, and the writer re-encodes from the rounded mm value. Vertices off the integer-coord grid lose precision on round-trip.
- **[round-trip | read]** `RawFlags (unmodelled flag bits)` — AltiumSharp preserves the raw 16-bit primitive flags word (PcbRegion.RawFlags) so unmodelled flag bits
    round-trip verbatim. Our read_flags (reader/mod.rs:334) decodes only LOCKED/KEEPOUT/TENTING_TOP/TENTING_BOTTOM and discards the rest of the word; the writer
    reconstructs from only those bits, so any other set bit on a real region is lost. Round-trip/byte fidelity.
- **[feature-loss | tool]** `kind / net / name on write schema` — Even setting aside the missing struct fields, the write_pcblib region schema (tool_definitions.rs:237)
    accepts only vertices and layer, and parse_region (parsing.rs:271) ignores any other JSON key. So an AI cannot specify a region's kind (copper vs cutout), net, or
    name when creating a footprint - the highest-value authoring gap that compounds the read-side losses above.
- **[feature-loss | tool]** `flags / holes / unique_id on write schema` — The write_pcblib region schema exposes neither flags (locked/keepout/tenting), holes, nor
    unique_id, and parse_region forces flags=empty, holes=Vec::new(), unique_id=None. These fields ARE in the struct and ARE produced on read, but an AI authoring a
    footprint cannot set a region as keepout/locked, cannot define cutout holes, and cannot assign a unique_id - so e.g. a keepout region cannot be created via the tool
    despite the model supporting it.

### Fill (gaps: 6)

Audited PcbLib Fill across our Rust (struct in src/altium/pcblib/primitives/text.rs lines 94-169; reader parse_fill in src/altium/pcblib/reader/parsers.rs lines
1007-1064; writer encode_fill_block in src/altium/pcblib/writer.rs lines 1081-1107; read JSON in src/mcp/tools/read_write.rs line 338 = raw serde of Fill; write schema in
src/mcp/tool_definitions.rs) vs AltiumSharp ground truth (binary reader ReadFill at C:/tmp/AltiumSharp/src/.../Serialization/Readers/PcbLibReader.cs lines 1542-1581;
model PcbFill.cs). Sanity-checked against golden TestData/Generated/Individual/PCB/FILLS_TEST.PcbLib (FILL records size=50: layer@0, flags@1-2, net@3=0xFFFF,
comp@7=0xFFFF, corners@13-28, rotation@29, sme@37, keepout@46). Geometry (x1,y1,x2,y2), layer and rotation are fully read+written+tool-exposed. The remaining
Altium-stored fields that ReadFill actually extracts are NOT fully handled, detailed in gaps. Note: the dozens of extra PcbFill.cs properties (TearDrop, ReliefAirGap,
PowerPlaneClearance, IsTestpoint*, etc.) are model defaults that ReadFill never populates from the byte stream, so they are not byte-stored fields for footprint fills and
are not counted as gaps.

- **[feature-loss | read]** `NetIndex (byte offset 3-4, u16)` — AltiumSharp ReadFill reads fill.NetIndex = U16(3) (0xFFFF = no net); golden FILLS_TEST stores it at
    offset 3. Our parse_fill (parsers.rs ~1023-1062) never reads offset 3-4 and the Fill struct has no net/net_index field, so the fill's net assignment is dropped on
    read. The writer hardcodes bytes 3-12 to 0xFF (write_common_header, writer.rs 256-258), so any net is lost. EE-meaningful: a copper fill on a signal layer can belong
    to a net; an AI cannot read or preserve which net a fill connects to.
- **[round-trip | read]** `ComponentIndex (byte offset 7-8, u16)` — AltiumSharp ReadFill reads ComponentIndex = U16(7) (0xFFFF/-1 = free). Our parse_fill never reads
    offset 7-8 and there is no struct field; the writer hardcodes it to 0xFF. For a PcbLib footprint this is normally 0xFFFF (free primitive), so it is round-trip
    fidelity rather than an AI-facing feature, but a non-default component index would be lost.
- **[feature-loss | tool]** `solder_mask_expansion` — The Fill struct has solder_mask_expansion (text.rs 117-124), the reader sets it from offset 37 (parsers.rs 1048) and
    the writer emits it at tail offset 37-40 (writer.rs 1102), so it survives a read-modify-write round-trip. But the write_pcblib create schema (tool_definitions.rs
    182-316) has NO fills array at all, and the update_primitive schema (1390-1405) exposes only x/y/width/height/rotation/layer for fill — there is no
    solder_mask_expansion key. An AI therefore cannot SET a fill's solder-mask expansion override via either tool, even though the model supports it. On read it IS
    exposed (raw serde of Fill includes solder_mask_expansion when present).
- **[feature-loss | tool]** `keepout_restrictions` — The Fill struct has keepout_restrictions (text.rs 125-127), read from offset 46 (parsers.rs 1049) and written at tail
    offset 46 (writer.rs 1104) — round-trips. But neither tool accepts it on write: no fills array in the write_pcblib create schema, and update_primitive's updates
    object (tool_definitions.rs 1390-1405) has no keepout_restrictions key. An AI cannot set the per-object keepout restriction bitmask (via/track/copper/SMD-pad/TH-pad)
    on a fill. It is exposed on read.
- **[feature-loss | tool]** `flags (IsLocked / IsTentingTop / IsTentingBottom / IsKeepout)` — Altium stores these in the flag word at bytes 1-2; AltiumSharp DecodeFlags
    splits them into IsLocked/IsTentingTop/IsTentingBottom/IsKeepout. Our reader decodes them into PcbFlags (read_flags, reader/mod.rs 334-353) and the writer re-encodes
    them, so they round-trip in the struct. On READ they are exposed only as an opaque transparent u16 integer (PcbFlags is #[serde(transparent)] and skipped when empty),
    so the AI sees e.g. flags=12 with no named locked/keepout/tenting booleans. On WRITE there is no way to set them: the create schema has no fills array and
    update_primitive exposes no flags/locked/keepout/tenting key. So an AI cannot meaningfully read (named) or set the fill's locked, keepout, or solder-mask-tenting
    state.
- **[feature-loss | tool]** `whole Fill primitive (create path)` — The primary authoring tool write_pcblib's footprint schema (tool_definitions.rs 170-317) lists pads,
    tracks, arcs, regions, text, step_model, component_bodies — but NO fills array. So an AI cannot CREATE any Fill primitive when writing a footprint; the only access is
    update_primitive, which can mutate an existing fill's x/y/width/height/rotation/layer but cannot add one. The model (Footprint.fills, add_fill), reader, and writer
    all fully support Fill, so this is purely a missing tool-schema input.

### PcbLib Text (gaps: 16)

Audited the PcbLib Text primitive across our model (src/altium/pcblib/primitives/text.rs struct, reader parsers.rs parse_text, writer.rs encode_text_geometry), the MCP
tools (read_pcblib serialises the whole Text struct at read_write.rs:339; write_pcblib schema at tool_definitions.rs:258-273 + parse_text at parsing.rs:304-338), and
Altium ground truth (AltiumSharp PcbText.cs / PcbTextDto.cs / PcbLibReader.ReadText, cross-checked against the golden TEXT_STROKE/TEXT_WIN1252 Data streams which confirm
the fixed 252-byte SubRecord-1 layout). Our struct models x,y,text,height,layer,rotation,kind,stroke_font,italic,stroke_width,justification,flags,unique_id. Coverage is
NOT complete. Two classes of gap: (1) fields our struct already has but the WRITE tool can't set because parse_text hardcodes them (kind, stroke_font, italic,
justification, flags) — an AI can read them back but never author non-default values; (2) Altium-stored fields our model omits entirely (mirror, bold, font_name,
net/component index, inverted-rectangle text-box block, barcode block, frame/snap tail, char_set, union_index, comment/designator flags) — the reader drops them and the
writer emits fixed template bytes, so a loaded Altium text loses them. The highest-value feature-loss gaps are the text mirror flag, TrueType font_name/bold,
justification, and authoring kind/stroke_font/italic via the write tool. Note: justification is the worst case — it is in the struct (and read JSON) but is never
populated by the reader, never emitted by the writer, AND not accepted by the write tool, so it is dead.

- **[feature-loss | read]** `mirror (IsMirrored, SubRecord-1 offset 35)` — Altium stores a per-text mirror flag at byte 35 (AltiumSharp PcbText.IsMirrored, read via
    B(35)). Our Text struct has no mirror field at all, parse_text never reads it, and encode_text_geometry never writes offset 35 (the TEXT_SR1_TEMPLATE byte 35 is 0x00
    and is left untouched). An AI cannot read or set whether a silkscreen text is mirrored (bottom-side text), and a loaded mirrored Altium text reads back as
    non-mirrored. Real EE-meaningful property.
- **[feature-loss | tool]** `kind (TextKind: TrueType/BarCode)` — The Text struct has `kind` (Stroke/TrueType/BarCode), the reader populates it from offset 160, the
    writer emits it (offset 160 + base@43), and read_pcblib serialises it — but the write_pcblib input schema (tool_definitions.rs:263-271) does not list `kind` and
    parse_text (parsing.rs:330) hardcodes `kind: TextKind::Stroke`. An AI can SEE a text's kind on read but can never author TrueType or BarCode text via the write tool.
- **[feature-loss | read]** `font_name (FontName, UTF-16 offsets 46-109)` — Altium stores the TrueType font name as a 64-byte UTF-16 field at offset 46 (PcbText.FontName;
    golden shows 'Arial'). Our model has no font_name field: parse_text/struct drop it, and encode_text_geometry always emits the template's hardcoded 'Arial' UTF-16
    bytes regardless of the actual font. TrueType text loses its font entirely on read, and an AI cannot choose a TrueType font on write. Pairs with the `kind` gap.
- **[feature-loss | read]** `font bold (FontBold, offset 44)` — Altium stores a bold flag at offset 44 (PcbText.FontBold, read via B(44)). Our struct models only `italic`
    (offset 45); there is no `bold` field. parse_text never reads it, the writer never sets offset 44 (template byte = 0x00). Bold TrueType text loses its weight on read
    and cannot be authored. (Italic IS handled; bold is the missing twin.)
- **[feature-loss | tool]** `justification (text-box anchor, offset 132)` — The struct has `justification` and read_pcblib serialises it, but it is dead end-to-end: the
    reader hardcodes TextJustification::default() (parsers.rs:725, deliberately not reading offset 132), the writer never emits offset 132, parse_text hardcodes
    MiddleCenter, and the write schema omits it. Altium stores the text-box justification at byte 132 (InvertedRectJustification, 1-9). An AI can neither read the real
    value nor set it. Tagged `tool` because it is in the struct yet unsettable; it is also a read+write gap.
- **[feature-loss | tool]** `flags / locked / keepout / tenting` — PcbFlags (LOCKED, KEEPOUT, TENTING_TOP/BOTTOM) round-trips through the reader (read_flags) and writer
    (encode_altium_flags) and is serialised in read_pcblib, but the write tool cannot set it: the write_pcblib schema exposes no flags/locked/keepout fields and
    parse_text hardcodes `flags: PcbFlags::empty()` (parsing.rs:335). An AI can read whether a text is locked/keepout but can never author those states.
- **[feature-loss | tool]** `stroke_font (non-default stroke font selection)` — The struct has `stroke_font` (Default/SansSerif/Serif), the reader populates it from the
    font-table id at offset 25 and the writer emits it, and read_pcblib serialises it — but the write schema omits it and parse_text hardcodes `stroke_font: None`
    (parsing.rs:331). An AI cannot select Sans-Serif/Serif stroke fonts when authoring text.
- **[feature-loss | tool]** `italic (FontItalic, offset 45) — write tool` — The struct has `italic`, the reader reads offset 45 and the writer emits it, and read_pcblib
    serialises it — but the write_pcblib schema does not list `italic` and parse_text hardcodes `italic: false` (parsing.rs:333). An AI can read italic state but cannot
    set it when creating text.
- **[round-trip | read]** `net_index (offset 3-4) / component_index (offset 7-8)` — Altium stores NetIndex@3 and ComponentIndex@7 in the common header
    (PcbText.NetIndex/ComponentIndex). Our reader's read_flags only decodes the flag word; bytes 3-12 are dropped (the writer hardcodes them to 0xFF). For a free library
    text these are 0xFFFF/none so it round-trips, but a text that carries a net/component association loses it on read. Mostly round-trip fidelity in a .PcbLib context.
- **[feature-loss | read]** `inverted text-box block (IsInverted@110, InvertedBorder@111, UseInvertedRectangle@123, InvertedRectWidth@124, InvertedRectHeight@128,
    InvertedRectTextOffset@133)` — Altium stores a full inverted-rectangle / text-frame description (knockout text on a filled rectangle) across offsets 110-133
    (ReadText). None of these are in our struct; the reader drops them and the writer emits the template defaults. Inverted (white-on-dark) silkscreen text is an
    EE-meaningful style that is lost on read and cannot be authored. Counted as one gap for the whole inverted-rect block.
- **[feature-loss | read]** `barcode block (BarCodeKind@157, BarCodeRenderMode@158, BarCodeInverted@159, BarCodeFontName@161-224, BarCodeShowText@225,
    BarCodeFullWidth/Height@137/141, BarCodeXMargin/YMargin@145/149, BarCodeMinWidth@153)` — Altium stores a complete barcode descriptor at offsets 137-225 (symbology,
    render mode, margins, show-text, barcode font name). Our model has no barcode fields and `kind: BarCode` cannot even be set, so barcode text is wholly unsupported:
    dropped on read, impossible to author. Counted as one gap for the barcode block.
- **[round-trip | read]** `frame/snap tail (IsFrame@230, IsOffsetBorder@231, IsJustificationValid@240, AdvanceSnapping@241, SnapPointX@244, SnapPointY@248)` — The 230-251
    tail (text-box frame mode, border-spacing mode, justification-valid flag, advance-snapping, and snap point) is read by AltiumSharp but absent from our model; the
    reader drops it and the writer replays the template tail. Mostly byte/round-trip fidelity for loaded files, though IsFrame (text rendered with a box) edges toward
    feature-loss.
- **[round-trip | read]** `char_set (offset 42)` — Altium stores the character-set byte at offset 42 (PcbText.CharSet). Our reader drops it and the writer leaves the
    template default (0). Loaded text with a non-default charset round-trips to 0. Round-trip fidelity.
- **[round-trip | read]** `union_index (offset 119)` — Altium stores the union/group index at offset 119 (PcbText.UnionIndex). Not modelled; reader drops it, writer emits
    template default. A grouped text loses its union membership on read. Round-trip fidelity.
- **[round-trip | read]** `is_comment (offset 40) / is_designator (offset 41)` — Altium flags whether the text is the component Comment or Designator field at offsets
    40/41 (PcbText.IsComment/IsDesignator). Our model derives nothing from these and drops them on read; the writer leaves template 0. The MCP infers '.Designator' from
    the text content string instead, so a designator text written by the tool does not set these bytes. Round-trip fidelity (the special-string content still works).
- **[round-trip | read]** `unique_id (UNIQUEID, PrimitiveGuids stream)` — The Text struct has `unique_id` and read_pcblib serialises it (skip_if_none), but parse_text
    always builds it as None and parse_text (write) never reads it, and the reader sets `unique_id: None` for every text (parsers.rs:757) — the per-primitive GUID from
    the PrimitiveGuids stream is never associated with the text. So a text's UNIQUEID is dropped on read and cannot be set on write. Round-trip / identity fidelity.

### ComponentBody (gaps: 22)

Audited PcbLib ComponentBody coverage across our struct (src/altium/pcblib/primitives/models3d.rs), reader (parse_component_body in
src/altium/pcblib/reader/parsers.rs:1073), writer (src/altium/pcblib/writer.rs build_component_body_params:1210), and MCP tools (read serialises fp.component_bodies
directly in src/mcp/tools/read_write.rs:341; write schema in src/mcp/tool_definitions.rs:288-314 + handler at read_write.rs:655-708), versus AltiumSharp ground truth
(C:/tmp/AltiumSharp PcbComponentBody.cs + ReadComponentBody at PcbLibReader.cs:1724). Golden sanity-checked via olefile against scripts/samples/footprints.PcbLib
BODY3D/Data.\n\nTwo classes of gaps dominate:\n\n1) TOOL gaps (highest AI-facing value): the read JSON exposes the full struct (serde), but the write_pcblib schema
accepts only overall_height, standoff_height, outline, layer, z_offset, rotation_x/y/z, model_checksum. The write handler (read_write.rs:679-706) HARD-CODES every other
struct field, so an AI cannot set, and a read-modify-write SILENTLY RESETS: body_color_3d, body_opacity_3d, body_projection, model_2d_rotation, is_shape_based, kind,
name, sub_poly_index, union_index, and model_id/model_name/embedded (the generic body path cannot link a STEP model). These fields ARE read and written by the lower
layers, but the tool boundary drops them.\n\n2) READ gaps (struct simply lacks the field, reader ignores the key, writer emits a fixed literal): Layer is the worst -
parse_v7_layer maps only MECHANICAL2-7 and collapses everything else (incl. the golden's MECHANICAL13) to Top3DBody, even though the enum/writer fully support
Mechanical1-32. Plus cavity_height, model_2d_location (MODEL.2D.X/Y), model_type, model_source, model_extruded_min/max_z, identifier, the texture set, both arc_resolution
values, net/component index, the lock/keepout/tenting flags, and AdditionalParameters (unmodelled trailing keys) are all dropped on read and re-synthesised as constants
by the writer.\n\nNOTE: model_checksum is the one previously-missing field that is now fully handled (read + written + tool-exposed) and is correctly NOT reported.
unique_id is a dead field (struct has it, reader always sets None, serde skips it). Coverage is NOT complete.

- **[feature-loss | read]** `Layer (V7_LAYER / layer byte)` — Our reader resolves the body's layer ONLY via parse_v7_layer() (parsers.rs:1241), which maps just 6 names
    (MECHANICAL2-7) and falls back to Top3DBody for everything else. It never reads the CommonPrimitiveData layer byte. The committed golden BODY3D/Data body is on
    MECHANICAL13 (layer id 69); on read it is silently collapsed to Top3DBody, so the body's true layer is lost. AltiumSharp sets result.Layer = layer (from the header
    byte) AND result.LayerName = V7_LAYER. Our Layer enum and writer CAN represent/emit Mechanical1-32, so this is purely a reader bug: extend parse_v7_layer to all
    MECHANICALn (and/or read the layer byte).
- **[feature-loss | tool]** `is_shape_based (ISSHAPEBASED)` — Field exists in struct, is read, and is written. But the write_pcblib schema (tool_definitions.rs:293-313)
    does not list it and the write handler (read_write.rs:702) hard-codes is_shape_based:false. An AI cannot create a shape-based body via the tool; on read-modify-write
    a shape-based body read from a file is reset to false because the write path rebuilds the struct from JSON and ignores any is_shape_based the read echoed.
- **[feature-loss | tool]** `body_color_3d (BODYCOLOR3D)` — Exposed on read (serde) and written, but NOT in the write schema; the write handler hard-codes
    body_color_3d:8_421_504 (read_write.rs:704). An AI cannot set a body's 3D colour, and a non-default colour read from a file is reset to grey on any read-modify-write.
- **[feature-loss | tool]** `body_opacity_3d (BODYOPACITY3D)` — Exposed on read and written, but absent from the write schema; the handler hard-codes body_opacity_3d:1.0
    (read_write.rs:705). An AI cannot make a body translucent, and a non-default opacity is lost on read-modify-write.
- **[feature-loss | tool]** `model_2d_rotation (MODEL.2D.ROTATION)` — Exposed on read and written, but not in the write schema; the handler hard-codes
    model_2d_rotation:0.0 (read_write.rs:706). An AI cannot set the model's 2D placement rotation, and a non-zero value is dropped on read-modify-write.
- **[feature-loss | tool]** `body_projection (BODYPROJECTION)` — Exposed on read and written, but not in the write schema; the handler hard-codes body_projection:0
    (read_write.rs:703). An AI cannot set the projection mode, and a non-default value is lost on read-modify-write.
- **[feature-loss | tool]** `model_id / model_name / embedded (MODELID, MODEL.NAME, MODEL.EMBED)` — All three are read and written, but the write schema only documents
    extruded bodies and the handler hard-codes model_id:'', model_name:'', embedded:false (read_write.rs:680-682). The dedicated external-STEP branch
    (read_write.rs:613-650) supports a model reference, but the generic component_bodies[] write path cannot attach/link a STEP model. A model-backed body read from a
    file (MODELID/MODEL.NAME/MODEL.EMBED populated) is downgraded to a model-less extruded body on read-modify-write.
- **[feature-loss | tool]** `kind (KIND)` — Read and written, but not accepted by the write schema; handler hard-codes kind:0 (read_write.rs:699). An AI cannot choose the
    body kind, and a non-zero KIND read from a file is reset on read-modify-write.
- **[round-trip | tool]** `name (NAME)` — Read and written, but not in the write schema; handler hard-codes name:' ' (read_write.rs:698). A custom body name read from a
    file is overwritten with the default space on read-modify-write through component_bodies[].
- **[round-trip | tool]** `sub_poly_index / union_index (SUBPOLYINDEX, UNIONINDEX)` — Both are read and written but not exposed by the write schema; handler hard-codes
    sub_poly_index:-1 and union_index:0 (read_write.rs:700-701). UNIONINDEX groups primitives that move together; a non-default value read from a file is lost on
    read-modify-write and an AI cannot set grouping.
- **[round-trip | read]** `unique_id (UniqueId)` — Struct has unique_id: Option<String>, but parse_component_body always sets it to None (parsers.rs:1173) - the reader
    never extracts a body UniqueId. AltiumSharp models PcbComponentBody.UniqueId. Because it serialises with skip_serializing_if=None it also never appears in read JSON,
    so it is invisible and unsettable end-to-end (dead field).
- **[feature-loss | read]** `cavity_height (CAVITYHEIGHT)` — AltiumSharp reads result.CavityHeight = Mil('CAVITYHEIGHT'). Our struct has no cavity_height field; the
    reader ignores the key and the writer always emits the literal 'CAVITYHEIGHT=0mil' (writer.rs:1240). A non-zero cavity height (embedded-component bodies) is lost on
    read and cannot be set.
- **[feature-loss | read]** `model_2d_location (MODEL.2D.X / MODEL.2D.Y)` — AltiumSharp reads result.Model2DLocation from MODEL.2D.X / MODEL.2D.Y. Our struct has no such
    field; the reader drops both keys and the writer always emits 'MODEL.2D.X=0mil|MODEL.2D.Y=0mil' (writer.rs:1281-1282). A body whose model is offset in the 2D plane
    loses that offset on read and an AI cannot set it.
- **[round-trip | read]** `model_type (MODEL.MODELTYPE)` — AltiumSharp reads result.ModelType = Int('MODEL.MODELTYPE'). We have no field; the reader ignores it and the
    writer DERIVES it (0 extruded / 1 model) from whether model_name is empty (writer.rs:1289). For a body whose stored MODELTYPE disagrees with that heuristic (e.g. a
    model-less type-1 body) the value is not preserved.
- **[round-trip | read]** `model_source (MODEL.MODELSOURCE)` — AltiumSharp reads result.ModelSource (nullable). We drop it on read and the writer hard-codes
    'MODEL.MODELSOURCE=Undefined' for non-extruded bodies (writer.rs:1304). A body with a real MODELSOURCE string is rewritten as Undefined on read-modify-write.
- **[round-trip | read]** `model_extruded_min_z / model_extruded_max_z (MODEL.EXTRUDED.MINZ/MAXZ)` — AltiumSharp reads ModelExtrudedMinZ/MaxZ. We don't store them; the
    reader ignores both, and the writer re-derives them from standoff_height/overall_height for extruded bodies only (writer.rs:1295-1302). If the stored MIN/MAXZ differ
    from standoff/overall (independent extrusion bounds) the originals are lost.
- **[round-trip | read]** `identifier (IDENTIFIER)` — AltiumSharp reads/decodes IDENTIFIER (comma-separated codepoints). Our reader never reads it and the writer always
    emits the empty literal 'IDENTIFIER=' (writer.rs:1258). A body with a non-empty identifier loses it on read-modify-write.
- **[round-trip | read]** `texture / texture_center_x/y / texture_size_x/y / texture_rotation (TEXTURE*)` — AltiumSharp models the full texture set (Texture,
    TextureCenterX/Y, TextureSizeX/Y, TextureRotation). Our reader drops all of them and the writer emits fixed literals (TEXTURE=, TEXTURECENTERX/Y=0mil,
    TEXTURESIZEX/Y=0.0001mil, TEXTUREROTATION= 0.0E+0; writer.rs:1259-1264). Any textured body loses its texture mapping on read-modify-write. Note the writer's
    TEXTURESIZE default (0.0001mil) does not even match the committed golden (0mil), so a fidelity mismatch already exists.
- **[round-trip | read]** `arc_resolution_prefix / arc_resolution_body (the two ARCRESOLUTION keys)` — AltiumSharp captures both duplicate ARCRESOLUTION occurrences
    (prefix + body) via the ordered parse. Our reader ignores ARCRESOLUTION entirely and the writer emits two fixed 'ARCRESOLUTION=0.5mil' literals (writer.rs:1235,1252).
    A body with a non-0.5mil arc resolution loses it on read-modify-write.
- **[round-trip | read]** `net_index (NetIndex) / component_index (ComponentIndex)` — AltiumSharp reads NetIndex and ComponentIndex from the CommonPrimitiveData header
    (ReadComponentBody sets result.NetIndex/ComponentIndex). Our parser skips the common header net/component fields for ComponentBody (no struct fields), so these are
    dropped on read. EE-relevance is low for library footprints (net is almost always 0xFFFF), hence round-trip rather than feature-loss.
- **[round-trip | read]** `flags: is_locked / is_keepout / is_tenting_top / is_tenting_bottom` — AltiumSharp decodes the primitive flags byte into IsLocked, IsKeepout,
    IsTentingTop, IsTentingBottom for the body. Our ComponentBody struct has no flags fields and the reader does not decode them, so lock/keepout/tenting state is lost on
    read. (is_keepout in particular is arguably feature-loss, but for a 3D body in a footprint library it is rarely meaningful, so classed round-trip.)
- **[round-trip | read]** `additional_parameters (trailing indexed keys, e.g. FPARTDRCDISABLED.N.n)` — AltiumSharp preserves any unmodelled trailing parameters in
    AdditionalParameters for round-trip fidelity. Our reader discards every key it does not explicitly look up, so any extra/vendor parameters in a real body are silently
    dropped on read and never re-emitted.

### SchLib Pin (gaps: 14)

Audited SchLib Pin (binary record type 2) across our Rust model and the AltiumSharp ground truth. Our struct/reader/writer cover the CORE binary record (name, designator,
x, y, length, orientation, electrical_type, hidden, show_name, show_designator, description, owner_part_id, colour, graphically_locked, is_not_accessible, the 4 symbol
edges, formal_type, swap_id_group, part_and_sequence, default_value) end-to-end with full round-trip. Ground truth: AltiumSharp's ReadBinaryPinRecord
(C:/tmp/AltiumSharp/.../Readers/SchLibReader.cs:470) decodes the binary record; SchPinDto.cs is the full field list; the SchLib binary pin also carries two auxiliary
streams (PinFrac for fractional coords, PinSymbolLineWidth for SymbolLineWidth). Gaps fall in three buckets: (1) one binary record field our reader skips and writer fakes
(owner_part_display_mode); (2) two auxiliary-stream fields we have no field for (symbol_line_width, fractional coords); (3) several struct fields the WRITE tool can't set
because parse_schlib_pin (src/mcp/tools/parsing.rs:345) hardcodes them and tool_definitions.rs (line 357-372) doesn't expose them — even though they ARE read-exposed and
round-tripped. Note: DTO text-only fields (custom font/colour/position modes, swap pairs, hidden-net-name, pin-package-length, propagation-delay) are NOT populated by the
binary SchLib reader, so they are not SchLib-physical gaps; excluded.

- **[round-trip | read]** `owner_part_display_mode (OWNERPARTDISPLAYMODE)` — AltiumSharp reads this 1 byte (SchLibReader.cs:485, ownerPartDisplayMode) into the SchPin.
    Our reader src/altium/schlib/reader.rs:280-281 just advances the offset (`offset += 1`) and never stores it; the Pin struct has no field for it. The writer
    src/altium/schlib/writer.rs:179 hardcodes 0x00. For multi-display-mode (de Morgan / alternate-view) parts a pin can belong to display mode 1+, and that byte is
    silently reset to 0 on every round-trip. EE-relevant only for components using alternate display modes, hence round-trip rather than feature-loss.
- **[feature-loss | read]** `symbol_line_width (SYMBOL_LINEWIDTH, PinSymbolLineWidth aux stream)` — Altium stores per-pin symbol line width in the separate
    `PinSymbolLineWidth` OLE stream; AltiumSharp parses it (SchLibReader.cs ApplyPinSymbolLineWidths -> SchPin.SymbolLineWidth) and SchPinDto exposes SYMBOL_LINEWIDTH.
    Our code has no struct field, never reads the stream, and never writes it (confirmed: grep for PinSymbolLineWidth across src/ = no matches; sample symbols.SchLib has
    no such stream). The thickness of pin decoration graphics (clock wedge, inversion bubble, etc.) is lost; an AI cannot read or set it.
- **[round-trip | read]** `fractional coordinates (LOCATION.X_FRAC / LOCATION.Y_FRAC / PINLENGTH_FRAC, PinFrac aux stream)` — Altium can place pins on sub-DXP fractional
    offsets, stored in the `PinFrac` OLE stream; AltiumSharp reads it (SchLibReader.cs ParsePinFracData/ApplyPinFracData, CoordFromDxp(dto.LocationX, dto.LocationXFrac)).
    Our reader only takes the i16 integer X/Y/length (reader.rs:315-322) and the struct has only i32 x/y/length — no _FRAC fields, and the writer never emits a PinFrac
    stream. Any fractional component of a pin's position or length is dropped on read and cannot be reproduced. Round-trip fidelity for fractionally-placed pins; rare in
    practice.
- **[feature-loss | tool]** `description (DESCRIPTION)` — Field exists in the Pin struct, is read (reader.rs:291) and written (writer.rs:188), and IS exposed on read
    (read_schlib serialises symbol.pins). But the WRITE path can't set it: parse_schlib_pin (src/mcp/tools/parsing.rs:448) hardcodes `description: String::new()` and
    tool_definitions.rs pin schema (lines 361-370) has no `description` property. An AI creating/overwriting a symbol cannot give a pin a functional description (used for
    pin-level docs and netlist export).
- **[feature-loss | tool]** `colour (COLOR)` — Pin.colour is read (reader.rs:325) and written (writer.rs:236) and read-exposed, but parse_schlib_pin hardcodes `colour: 0`
    (parsing.rs:450) and the write schema omits it. An AI cannot set a non-default pin colour when authoring a symbol via write_schlib (e.g. to highlight power pins).
- **[round-trip | tool]** `formal_type (FORMALTYPE)` — Read (reader.rs:295) and written (writer.rs:191) and read-exposed, but parse_schlib_pin hardcodes `formal_type: 1`
    (parsing.rs:457) and it is absent from the write schema. An AI round-tripping an Altium-authored pin with a non-1 FORMALTYPE through write_schlib silently resets it
    to 1. Rarely EE-meaningful, so round-trip.
- **[feature-loss | tool]** `graphically_locked (GRAPHICALLYLOCKED / conglomerate bit 0x40)` — Read (reader.rs:311) and written (writer.rs:214) and read-exposed, but
    parse_schlib_pin hardcodes `graphically_locked: false` (parsing.rs:451) and the write schema omits it. An AI cannot set a pin as graphically locked (prevents
    accidental drag in the editor) when authoring via write_schlib, and an overwrite clears any existing lock.
- **[round-trip | tool]** `is_not_accessible (ISNOTACCESIBLE / conglomerate bit 0x20)` — Read (reader.rs:312) and written (writer.rs:217) and read-exposed, but
    parse_schlib_pin hardcodes `is_not_accessible: false` (parsing.rs:456) and it is absent from the write schema. An AI cannot set it and an overwrite clears it;
    primarily an editor-selection flag, so round-trip.
- **[feature-loss | tool]** `swap_id_group (SWAPIDGROUP)` — Read (reader.rs:341) and written (writer.rs:247) and read-exposed, but parse_schlib_pin hardcodes
    `swap_id_group: String::new()` (parsing.rs:458) and the write schema omits it. Pin-swap groups let Altium swap functionally-equivalent pins during layout (e.g. gate
    inputs); an AI cannot author them via write_schlib.
- **[feature-loss | tool]** `part_and_sequence (SwapIdPart + SwapIdSequence, the "{part}|&|{seq}" field)` — Read (reader.rs:343) and written (writer.rs:248) and
    read-exposed, but parse_schlib_pin hardcodes `part_and_sequence: "|&|"` (parsing.rs:459) and the write schema omits it. This encodes per-pin swap part/sequence IDs
    for pin-swapping; an AI cannot set non-default swap IDs, and overwriting a pin that had them resets to the empty default.
- **[feature-loss | tool]** `default_value (DEFAULTVALUE)` — Read (reader.rs:345) and written (writer.rs:249) and read-exposed, but parse_schlib_pin hardcodes
    `default_value: String::new()` (parsing.rs:460) and the write schema omits it. The pin default value (used for simulation/connectivity defaults, e.g. tie-off level)
    cannot be set by an AI via write_schlib and is cleared on overwrite.
- **[feature-loss | tool]** `symbol_inner_edge / symbol_outer_edge / symbol_inside / symbol_outside` — These four PinSymbol decorations ARE read (reader.rs:284-287),
    written (writer.rs:182-185), read-exposed, AND parsed from write JSON (parse_schlib_pin parsing.rs:420-435 accepts them). The gap is schema-only: tool_definitions.rs
    pin properties (lines 361-370) do NOT list symbol_inner_edge/outer_edge/inside/outside, so an AI reading the write_schlib schema has no way to discover that it can
    place an inversion dot/clock wedge/active-low bar etc. — a core EE feature (e.g. active-low CS pins, clock inputs). Functional but undiscoverable; documenting them in
    the schema would close it.
- **[cosmetic | tool]** `hidden / show_name / show_designator` — Accepted by parse_schlib_pin (parsing.rs:381-389) and fully read/written, but not declared in the
    tool_definitions.rs pin write schema. An AI can pass them but won't discover them from the schema. Lower impact than the symbol decorations since
    show_name/show_designator default sensibly and hidden pins are uncommon; schema-documentation gap only.
- **[feature-loss | tool]** `electrical_type enum coverage (open_collector / open_emitter / hiz)` — The PinElectricalType enum and parse_schlib_pin (parsing.rs:367-379)
    support input/output/bidirectional/passive/power AND open_collector/open_emitter/hiz/tristate. But the tool_definitions.rs write schema (line 368) advertises only
    enum [input, output, bidirectional, passive, power], omitting open_collector, open_emitter and hiz/tri-state. An AI constrained to the published enum cannot mark a
    pin as open-collector/open-emitter/high-Z — all real ERC-relevant electrical types.

### SchLib Rectangle (RECORD=14) (gaps: 10)

Audited SchLib Rectangle (RECORD=14) across our struct (src/altium/schlib/primitives.rs Rectangle), reader (src/altium/schlib/reader.rs parse_rectangle), writer
(src/altium/schlib/writer.rs encode_rectangle), read tool JSON (src/mcp/tools/read_write.rs:882, full-struct serde), and write schema/parser
(src/mcp/tool_definitions.rs:374 + src/mcp/tools/parsing.rs:466 parse_schlib_rectangle) vs the authoritative Altium field list (C:/tmp/AltiumSharp SchRectangleDto +
SchRectangle model + SchLibReader.CreateRectangle), sanity-checked against the committed golden scripts/samples/symbols.SchLib RECTS records. Coordinate/colour/fill
basics (x1-y2, line_width, line_color, fill_color, filled) are fully covered (read+write+tool). Found 10 gaps in three categories: TOOL gaps (field exists+read+written
but the write tool can't accept it — parse_schlib_rectangle hard-codes the value): line_style and transparent (both feature-loss — an AI can read a dashed/transparent
rectangle but cannot create or preserve one; round-trip forces solid+opaque), and unique_id (round-trip — write fabricates a new ID each rewrite). The write schema in
tool_definitions.rs lists only x1/y1/x2/y2/line_width/line_color/fill_color/filled/owner_part_id. READ gaps (Altium stores it, our struct/reader/writer have no field at
all): OwnerPartDisplayMode (feature-loss — alternate display-mode membership), GraphicallyLocked (feature-loss — user lock state; note Pin has this field but Rectangle
does not), Disabled, Dimmed, OwnerIndex, and the four corner *_FRAC sub-coordinates (all round-trip fidelity). One mixed case: IsNotAccessible is never read into the
struct and is hard-coded to =T on write (writer.rs:302), so a rectangle authored accessible (=F) round-trips as not-accessible. Most gaps surface only on Altium-authored
files with non-default flags (the golden's plain rectangles round-trip fine), but the two highest-value, fully-actionable items are line_style and transparent: the data
already flows through struct/reader/writer/read-JSON and is blocked solely by the write tool schema + parse_schlib_rectangle hard-coding — exposing them there would close
those two with no format work.

- **[feature-loss | tool]** `line_style (LINESTYLEEXT)` — line_style IS in the struct, IS read (parse_rectangle reads `linestyleext`), IS written (encode_rectangle emits
    LineStyleExt), and IS exposed on the read JSON (serde dumps the whole struct). But the WRITE path drops it: tool_definitions.rs has NO `line_style` property in the
    rectangle input schema, and parse_schlib_rectangle (src/mcp/tools/parsing.rs:494) hard-codes `line_style: 0`. So an AI can SEE a dashed/dotted rectangle on read but
    can never CREATE or PRESERVE one through write_schlib — any value it supplies is ignored and dashed borders silently become solid. Altium stores
    0=Solid,1=Dashed,2=Dotted,3=DashDot.
- **[feature-loss | tool]** `transparent (TRANSPARENT)` — transparent IS in the struct, read, written, and on the read JSON, but the write tool can't set it: no
    `transparent` property in tool_definitions.rs rectangle schema and parse_schlib_rectangle (parsing.rs:496) hard-codes `transparent: false`. An AI cannot author a
    transparent (non-opaque-fill) rectangle; a round-trip through write_schlib forces every rectangle opaque.
- **[round-trip | tool]** `unique_id (UNIQUEID)` — unique_id IS in the struct, read, written, and on the read JSON, but the write tool discards it: no
    `unique_id`/`uniqueid` property in tool_definitions.rs rectangle schema and parse_schlib_rectangle (parsing.rs:498) hard-codes `unique_id: None`. So an AI round-trip
    (read -> write_schlib) cannot feed the read UniqueID back; the writer fabricates a fresh 8-char ID (encode_rectangle: generate_unique_id), changing the primitive's
    stable identity on every rewrite.
- **[feature-loss | read]** `OwnerPartDisplayMode (OWNERPARTDISPLAYMODE)` — Altium stores OWNERPARTDISPLAYMODE on every rectangle (SchRectangleDto.OwnerPartDisplayMode;
    SchLibReader populates SchRectangle.OwnerPartDisplayMode). Our Rectangle struct has no field for it, parse_rectangle never reads it, and the writer never emits it. A
    rectangle that belongs to an alternate display mode (Alternate-1/2) is silently re-homed to the primary display mode on round-trip. EE-meaningful (de-Morgan /
    alternate symbol views).
- **[feature-loss | read]** `GraphicallyLocked (GRAPHICALLYLOCKED)` — Altium stores GRAPHICALLYLOCKED per rectangle (DTO + model field, populated by SchLibReader). Our
    struct/reader/writer have no equivalent (the Rectangle struct lacks the `graphically_locked` field that Pin has). A user-locked body rectangle loses its lock on read
    and on round-trip, so an AI can neither see nor preserve the locked state.
- **[round-trip | read]** `Disabled (DISABLED)` — Altium stores DISABLED per rectangle (SchRectangleDto.Disabled, mapped to SchRectangle.Disabled). Our struct has no
    field; parse_rectangle ignores it and the writer never emits it. A disabled rectangle round-trips as enabled (data lost on read, hence also on write).
- **[round-trip | read]** `Dimmed (DIMMED)` — Altium stores DIMMED per rectangle (SchRectangleDto.Dimmed -> SchRectangle.Dimmed). No corresponding struct field, reader,
    or writer support, so the dimmed-display flag is dropped on read and lost on round-trip.
- **[round-trip | read]** `OwnerIndex (OWNERINDEX)` — Altium stores OWNERINDEX (link to the parent owning record in the sheet hierarchy) on each rectangle
    (SchRectangleDto.OwnerIndex -> SchRectangle.OwnerIndex). Our struct/reader has no field for it and the writer never emits it. In flat SchLib symbols this is usually
    -1/absent, but any non-default owner linkage is dropped; pure read-side data loss.
- **[round-trip | read]** `Location/Corner _FRAC (LOCATION.X_FRAC, LOCATION.Y_FRAC, CORNER.X_FRAC, CORNER.Y_FRAC)` — Altium stores a fractional sub-coordinate companion
    for each corner ordinate (SchRectangleDto.LocationXFrac/LocationYFrac/CornerXFrac/CornerYFrac; reader uses CoordFromDxp(int, frac)). Our Rectangle stores x1/y1/x2/y2
    as plain i32 and parse_rectangle only reads the integer `location.x`/`corner.y` keys, dropping any *_FRAC part; the writer emits no _FRAC. A rectangle placed off the
    integer DXP grid loses its sub-unit precision on read and round-trip. (EllipticalArc already models *_frac; Rectangle does not.)
- **[round-trip | tool]** `IsNotAccessible (ISNOTACCESIBLE)` — Altium stores ISNOTACCESIBLE per rectangle (DTO bool, model IsNotAccessible). Our reader does NOT read it
    into the struct and the struct has no field; the writer instead HARD-CODES `IsNotAccesible=T` for every rectangle (encode_rectangle, writer.rs:302). So the real
    stored value is neither read nor preservable: a rectangle authored with IsNotAccesible=F (accessible) round-trips as =T. Tagged 'tool'/'round-trip' because it is
    silently forced rather than surfaced; lower EE impact than the display/lock flags above but still a fidelity gap.

### RoundRect (SchLib, RECORD=10) (gaps: 9)

Audited SchLib RoundRect across our Rust model, reader, writer, and MCP tools vs AltiumSharp's SchRoundedRectangleDto (authoritative, RECORD=10) and the golden
ROUNDRECTS/Data stream in scripts/samples/symbols.SchLib. OUR MODEL: struct RoundRect (src/altium/schlib/primitives.rs:732) has
x1,y1,x2,y2,corner_x_radius,corner_y_radius,line_width,line_color,fill_color,line_style,filled,transparent,owner_part_id,unique_id. The reader parse_round_rect
(src/altium/schlib/reader.rs:769) populates EVERY struct field (no struct field is left unset). The writer encode_round_rect (src/altium/schlib/writer.rs:545) emits all
of them, so every modelled field round-trips. The read tool (src/mcp/tools/read_write.rs:883) serialises symbol.round_rects via serde, exposing all struct fields. BIGGEST
GAP: RoundRect is entirely UNWRITABLE through the MCP. The write_schlib input schema (src/mcp/tool_definitions.rs ~341-471) defines only pins, rectangles, lines, text,
parameters, footprints under each symbol -- there is NO round_rects property. And the write handler (src/mcp/tools/read_write.rs:1023-1158) parses
pins/rectangles/lines/polylines/arcs/ellipses/labels/text/parameters/footprints but never parses round_rects. So an AI can READ round_rects but cannot CREATE or modify
any RoundRect via this MCP. ALTIUM SUPERSET (from SchRoundedRectangleDto): besides the fields we model, Altium also stores OWNERINDEX, OWNERPARTDISPLAYMODE,
GRAPHICALLYLOCKED, DISABLED, DIMMED, and fractional companions LOCATION.X_FRAC/Y_FRAC, CORNER.X_FRAC/Y_FRAC, CORNERXRADIUS_FRAC, CORNERYRADIUS_FRAC. Our struct/reader
handle none of these (coords are plain i32; the _FRAC keys are read for EllipticalArc but never for RoundRect). The golden sample uses only the common subset we already
cover.

- **[feature-loss | tool]** `round_rects (entire primitive) in write_schlib` — RoundRect is fully readable but completely unwritable via the MCP. The write_schlib input
    schema (src/mcp/tool_definitions.rs, symbol 'properties' ~lines 351-461) lists only pins/rectangles/lines/text/parameters/footprints and has no 'round_rects'
    property; the write handler (src/mcp/tools/read_write.rs:1023-1158) parses pins, rectangles, lines, polylines, arcs, ellipses, labels, text, parameters, footprints
    but never reads sym_json['round_rects']. Result: an AI can read existing rounded rectangles but cannot author or round-trip-modify any RoundRect. The
    model+reader+writer all support it; only the write tool layer is missing. Highest-value gap.
- **[feature-loss | read]** `GraphicallyLocked` — AltiumSharp SchRoundedRectangleDto exposes GRAPHICALLYLOCKED (bool). Our RoundRect struct has no field and
    parse_round_rect never reads 'graphicallylocked', so the lock state is dropped on read and lost on write. EE-meaningful: locking a graphic prevents accidental edits
    in Altium.
- **[feature-loss | read]** `Disabled` — DISABLED (bool) in the Altium DTO. No struct field and not read by parse_round_rect; a disabled rounded rectangle is silently
    re-enabled on round-trip and the flag is invisible to the AI.
- **[feature-loss | read]** `Dimmed` — DIMMED (bool) in the Altium DTO. No struct field and not read by parse_round_rect; the dimmed display state is dropped on read and
    not preserved on write.
- **[feature-loss | read]** `OwnerPartDisplayMode` — OWNERPARTDISPLAYMODE (int) selects which alternate display mode/de-Morgan variant the primitive belongs to. Our
    struct lacks it and parse_round_rect never reads 'ownerpartdisplaymode' (always effectively mode 0). Multi-display-mode symbols lose the mode assignment on round-trip
    and the AI cannot place a RoundRect in an alternate display mode.
- **[round-trip | read]** `OwnerIndex` — OWNERINDEX (int) links the primitive to its parent record in the hierarchy. Not stored on the struct and not read; the writer
    also never emits OWNERINDEX. For component-scoped library primitives this is typically the implicit component owner, but the value is not preserved verbatim on
    read-modify-write.
- **[round-trip | read]** `Location.X_FRAC / Location.Y_FRAC` — Altium stores fractional sub-unit companions LOCATION.X_FRAC and LOCATION.Y_FRAC. RoundRect coords are
    plain i32 (x1,y1) and parse_round_rect uses coord() which reads only the integer 'location.x'/'location.y' keys, ignoring the _FRAC parts (the project does read _frac
    for EllipticalArc but not here). A corner placed off the integer grid loses its fractional offset on read and is not re-emitted on write.
- **[round-trip | read]** `Corner.X_FRAC / Corner.Y_FRAC` — Fractional companions CORNER.X_FRAC and CORNER.Y_FRAC for the second corner. x2,y2 are plain i32 read via
    coord() from 'corner.x'/'corner.y' only; the _FRAC keys are never read and never written, so sub-unit corner precision is lost on round-trip.
- **[round-trip | read]** `CornerXRadius_FRAC / CornerYRadius_FRAC` — Altium stores CORNERXRADIUS_FRAC and CORNERYRADIUS_FRAC for sub-unit corner-radius precision.
    corner_x_radius/corner_y_radius are i32 parsed from the integer keys only; parse_round_rect never reads the _FRAC keys and the writer never emits them, so a
    fractional corner radius is truncated on read-modify-write.

### Ellipse (SchLib) (gaps: 11)

Audited SchLib Ellipse coverage across struct (src/altium/schlib/primitives.rs:669 `Ellipse`), reader (src/altium/schlib/reader.rs:637 `parse_ellipse`), writer
(src/altium/schlib/writer.rs:518 `encode_ellipse`), read tool (src/mcp/tools/read_write.rs:888 serialises `symbol.ellipses` directly), write parser
(src/mcp/tools/parsing.rs:655 `parse_schlib_ellipse`) and write schema (src/mcp/tool_definitions.rs write_schlib). Ground truth = AltiumSharp SchEllipseDto/SchEllipse +
ELLIPSE_TEST golden (record type 8). Geometry (x, y, radius_x, radius_y), line_width, line_color (Color), fill_color (AreaColor), filled (IsSolid), transparent,
owner_part_id and unique_id are fully read, written and exposed on the READ tool. The biggest gap: the write_schlib input schema has NO `ellipses` property at all (only
the prose description mentions ellipses), so an AI cannot discover ellipses are writable from the schema — though the write parser still accepts them if supplied. Two
struct fields the read-tool exposes (transparent, unique_id) are silently dropped by the write parser (hardcoded transparent:false, unique_id:None). Six EE/identity
Altium properties have no struct field at all: OwnerIndex, IsNotAccessible, OwnerPartDisplayMode, GraphicallyLocked, Disabled, Dimmed. Sub-unit fractional coordinate
parts (LOCATION.X_FRAC, LOCATION.Y_FRAC, RADIUS_FRAC, SECONDARYRADIUS_FRAC) are dropped on read (parse_ellipse uses the integer-only `coord` helper, unlike
parse_elliptical_arc which does handle _FRAC).

- **[feature-loss | tool]** `ellipses (write_schlib input schema)` — src/mcp/tool_definitions.rs defines write_schlib properties for pins, rectangles, lines, arcs etc.
    but has NO `ellipses` array property — only the prose description string (line 335) mentions ellipses. The Ellipse struct, writer and write parser (parsing.rs:655
    parse_schlib_ellipse) all exist and work, but because the JSON Schema omits the field, an AI inspecting the tool definition cannot discover that ellipses are writable
    or learn their field names (x, y, radius_x, radius_y, line_width, line_color, fill_color, filled, owner_part_id). Add an `ellipses` items schema mirroring the struct.
- **[feature-loss | tool]** `transparent (write parser)` — Ellipse.transparent is read (reader.rs:657), written (writer.rs:522 emits |Transparent=T) and exposed on the
    read tool (read_write.rs:888 serialises the whole struct). But the write-side parser parse_schlib_ellipse (parsing.rs:684) hardcodes `transparent: false` and never
    reads json["transparent"]. An AI that round-trips an ellipse or sets transparent on write silently loses it. Read json["transparent"] like the reader does.
- **[round-trip | tool]** `unique_id (write parser)` — Ellipse.unique_id is read (reader.rs:674), written (writer.rs:540, preserved or freshly generated) and shown on the
    read tool. But parse_schlib_ellipse (parsing.rs:686) hardcodes `unique_id: None`, so feeding a read ellipse back into write_schlib discards the original 8-char
    UniqueID and a new one is generated, breaking shape-identity stability (#113). Parse json["unique_id"] on write.
- **[round-trip | read]** `OwnerIndex (OWNERINDEX)` — AltiumSharp SchEllipseDto/SchEllipse store OwnerIndex (parent-record link in the schematic hierarchy). Our Ellipse
    struct has no owner_index field; the reader never parses OWNERINDEX and the writer never emits it. Lost on read; absent from round-trip. Matters for primitives owned
    by nested records.
- **[round-trip | read]** `IsNotAccessible (ISNOTACCESIBLE)` — Altium stores IsNotAccessible (selection-accessibility flag) on the ellipse. No field in our struct; reader
    does not parse ISNOTACCESIBLE; writer does not emit it. Dropped entirely.
- **[feature-loss | read]** `OwnerPartDisplayMode (OWNERPARTDISPLAYMODE)` — Altium stores OwnerPartDisplayMode (which alternate display mode / De-Morgan view the
    primitive belongs to). Our struct lacks it, reader skips OWNERPARTDISPLAYMODE, writer omits it. An ellipse only meant to appear in an alternate display mode is
    silently flattened into the default mode.
- **[feature-loss | read]** `GraphicallyLocked (GRAPHICALLYLOCKED)` — Altium stores GraphicallyLocked (prevents the shape being moved/edited graphically). Our Ellipse
    struct has no field; reader and writer ignore it. An AI cannot read or preserve the lock state.
- **[round-trip | read]** `Disabled (DISABLED)` — Altium stores a Disabled flag on the ellipse. No struct field; reader does not parse DISABLED; writer does not emit it.
    Lost on read and round-trip.
- **[round-trip | read]** `Dimmed (DIMMED)` — Altium stores a Dimmed (display-dimming) flag. No struct field; reader skips DIMMED; writer omits it. Dropped entirely.
- **[round-trip | read]** `LOCATION.X_FRAC / LOCATION.Y_FRAC` — Altium stores fractional sub-unit parts of the centre coordinate (DTO LocationXFrac/LocationYFrac).
    parse_ellipse (reader.rs:638-639) uses the integer-only `coord` helper and ignores LOCATION.X_FRAC/LOCATION.Y_FRAC; the struct stores only i32 x/y; the writer never
    emits *_FRAC. Sub-unit centre precision is truncated on read (note: parse_elliptical_arc DOES handle _FRAC, so this is an inconsistency).
- **[round-trip | read]** `RADIUS_FRAC / SECONDARYRADIUS_FRAC` — Altium stores fractional sub-unit parts of both radii (DTO RadiusFrac/SecondaryRadiusFrac). parse_ellipse
    reads RADIUS/SECONDARYRADIUS as plain integers (reader.rs:640-645) and never reads the *_FRAC params; struct radius_x/radius_y are i32; writer emits no *_FRAC.
    Sub-unit radius precision is lost on read/round-trip, again unlike parse_elliptical_arc which handles radius_frac/secondaryradius_frac.

### SchLib Line (RECORD=13) (gaps: 10)

Audited the SchLib Line primitive across our MCP (struct src/altium/schlib/primitives.rs L433-483; reader parse_line src/altium/schlib/reader.rs L439-474; writer
encode_line src/altium/schlib/writer.rs L321-344; read tool src/mcp/tools/read_write.rs L884 serde-serialises symbol.lines; write schema src/mcp/tool_definitions.rs
L393-408; write parse parse_schlib_line src/mcp/tools/parsing.rs L504-531) vs Altium ground truth (C:/tmp/AltiumSharp SchLineDto.cs full param list + CreateLine
SchLibReader.cs L1253-1276). Our Line struct fields: x1,y1,x2,y2,line_width,color,line_style,is_not_accessible,owner_part_id,unique_id. Altium (SchLineDto) stores
additionally: OwnerIndex, OwnerPartDisplayMode, AreaColor, GraphicallyLocked, Disabled, Dimmed, and the *_FRAC sub-coord fields (Location.X_FRAC/Y_FRAC,
Corner.X_FRAC/Y_FRAC). Plus IndexInSheet (we recompute it positionally on write — not a real loss). Fully-covered (read+write+tool, NOT gaps): x1/y1/x2/y2 (integer
coords), line_width, color, owner_part_id. Three classes of gap found: (1) WRITE-TOOL gaps — line_style, is_not_accessible, unique_id ARE in the struct, are read, written
by the writer, and exposed on the read JSON, but the write input schema omits them and parse_schlib_line HARDCODES line_style=0 / is_not_accessible=true / unique_id=None,
so an AI cannot set them. (2) READ/struct gaps (Altium fields our struct never models, dropped on read AND therefore lost on round-trip): OwnerIndex,
OwnerPartDisplayMode, AreaColor, GraphicallyLocked, Disabled, Dimmed. (3) Round-trip fidelity gap: the four *_FRAC fractional sub-coordinates are not
parsed/stored/written, so sub-DXP-unit line endpoint precision is lost. Coverage is NOT complete.

- **[feature-loss | tool]** `line_style (LineStyle)` — Struct has line_style; reader parses 'linestyle'; writer emits LineStyle; read JSON exposes it via serde
    (symbol.lines). BUT the write_schlib input schema (tool_definitions.rs L398-405) omits it and parse_schlib_line (parsing.rs L526) hardcodes line_style:0. An AI can
    READ a line's dashed/dotted style but cannot SET it on write — any non-solid style is silently forced to solid.
- **[round-trip | tool]** `is_not_accessible (IsNotAccesible)` — Struct/reader/writer all handle it and read JSON exposes it, but the write schema omits it and
    parse_schlib_line (parsing.rs L527) hardcodes is_not_accessible:true. An AI authoring a line cannot set it false; round-trip through the write tool always re-tags
    lines IsNotAccesible=T regardless of input.
- **[round-trip | tool]** `unique_id (UniqueID)` — Struct/reader/writer preserve UniqueID and the read JSON exposes it, but the write schema omits it and
    parse_schlib_line (parsing.rs L529) hardcodes unique_id:None. Feeding read-tool output back through the write tool drops the original identity and the writer mints a
    fresh UniqueID, breaking stable primitive identity on round-trip.
- **[feature-loss | read]** `AreaColor` — Altium SchLineDto stores AREACOLOR and SchLine.AreaColor (the line's fill/area colour). Our Line struct has no fill/area-colour
    field, the reader never reads 'areacolor', and the writer never emits it. An AI cannot read or set it; the value is dropped on read and lost on every write.
- **[feature-loss | read]** `GraphicallyLocked` — Altium SchLineDto has GRAPHICALLYLOCKED (bool) and SchLine.GraphicallyLocked. Our Line struct lacks it, the reader skips
    'graphicallylocked', and the writer never emits it. A locked line is silently unlocked on round-trip and the AI can neither see nor set the lock state (other SchLib
    primitives, e.g. Pin, do model graphically_locked).
- **[round-trip | read]** `Disabled` — Altium SchLineDto has DISABLED (bool) and SchLine.Disabled. Our Line struct has no field, the reader skips it, the writer omits it.
    Dropped on read and lost on write.
- **[round-trip | read]** `Dimmed` — Altium SchLineDto has DIMMED (bool) and SchLine.Dimmed. Our Line struct has no field, the reader skips it, the writer omits it.
    Dropped on read and lost on write.
- **[feature-loss | read]** `OwnerPartDisplayMode` — Altium SchLineDto stores OWNERPARTDISPLAYMODE and SchLine.OwnerPartDisplayMode (alternate/de-Morgan display-mode
    selector). Our Line struct has no field, the reader skips it, the writer omits it. A line that belongs only to an alternate display mode loses that association on
    round-trip, and an AI cannot read or assign it.
- **[round-trip | read]** `OwnerIndex` — Altium SchLineDto stores OWNERINDEX and SchLine.OwnerIndex (parent-record hierarchy link). Our Line struct has no field, the
    reader skips 'ownerindex', the writer never emits it. Dropped on read; for nested/child ownership this hierarchy link is lost on round-trip.
- **[round-trip | read]** `Location.X_FRAC / Location.Y_FRAC / Corner.X_FRAC / Corner.Y_FRAC` — Altium stores fractional sub-coordinates (SchLineDto
    LocationXFrac/LocationYFrac/CornerXFrac/CornerYFrac; CreateLine combines them via CoordFromDxp(int, frac)). Our reader only reads integer Location/Corner via coord();
    the struct stores i32 x1/y1/x2/y2 with no frac, and the writer emits no *_FRAC. Sub-DXP-unit endpoint precision present in Altium-authored files is silently truncated
    on read and lost on write.

### SchLib Polyline (RECORD=6) (gaps: 15)

Audited SchLib Polyline coverage across our Rust model (src/altium/schlib/primitives.rs Polyline struct, reader.rs parse_polyline, writer.rs encode_polyline) and the MCP
tools (read = src/mcp/tools/read_write.rs serialising symbol.polylines; write schema = src/mcp/tool_definitions.rs; write parser = src/mcp/tools/parsing.rs
parse_schlib_polyline) versus Altium ground truth (AltiumSharp SchPolylineDto + SchLibReader.CreatePolyline at C:/tmp/AltiumSharp), sanity-checked against the golden
POLYLINES/Data in scripts/samples/symbols.SchLib. The binary reader/writer + struct already handle the visual core (points, line_width, color, line_style,
start/end_line_shape, line_shape_size, transparent, owner_part_id, unique_id) with full round-trip. The struct fields ARE all exposed on read (read_schlib serialises the
whole struct). Two distinct classes of gap remain: (1) genuinely-dropped Altium fields the struct lacks entirely (IsNotAccesible, AreaColor fill, IsSolid,
OwnerPartDisplayMode, GraphicallyLocked, Disabled, Dimmed, plus vertex _FRAC); and (2) the write tool: the write_schlib JSON schema omits a polylines property ENTIRELY,
and even the silently-accepted parse_schlib_polyline hardcodes line_style/start_line_shape/end_line_shape/line_shape_size/transparent rather than reading them from JSON —
so an AI cannot set fields the struct/writer already support. The golden confirms real Altium emits IsNotAcceesible=T on polylines, which we never read or write. Note:
the AltiumSharp fork here is OriginalCircuit's DTO-based variant; its SchPolylineDto is the authoritative parameter list for RECORD=6.

- **[feature-loss | tool]** `polylines (write_schlib input schema)` — src/mcp/tool_definitions.rs write_schlib schema defines
    pins/rectangles/lines/text/parameters/footprints but NO 'polylines' property at all (the only 'vertices' at line 243 belongs to the PcbLib write tool). The write
    parser in parsing.rs does accept a 'polylines'/'vertices' array, but because the documented schema omits it, an AI relying on the tool contract cannot discover that
    polylines are writable. Add a polylines item schema (points[{x,y}], line_width, color, line_style, start_line_shape, end_line_shape, line_shape_size, transparent,
    owner_part_id).
- **[feature-loss | tool]** `line_style (write parser)` — parse_schlib_polyline in src/mcp/tools/parsing.rs (line ~605) hardcodes line_style: 0 instead of reading json
    'line_style'. The struct field exists and writer.rs encode_polyline emits LineStyle, so dashed/dotted polylines round-trip on READ but an AI cannot SET them via
    write_schlib. Read json.get("line_style").
- **[feature-loss | tool]** `start_line_shape (write parser)` — parse_schlib_polyline hardcodes start_line_shape: 0. The struct/reader/writer fully support StartLineShape
    (arrow/tail endpoint decoration), but the write tool never reads it from JSON, so an AI cannot author a polyline start arrowhead.
- **[feature-loss | tool]** `end_line_shape (write parser)` — parse_schlib_polyline hardcodes end_line_shape: 0. EndLineShape (end-cap arrow/dot) is supported by
    struct/reader/writer but unreachable from write_schlib JSON.
- **[feature-loss | tool]** `line_shape_size (write parser)` — parse_schlib_polyline hardcodes line_shape_size: 0. The size of the start/end endpoint shapes
    (LineShapeSize) is supported by the struct/reader/writer but cannot be set via the write tool.
- **[round-trip | tool]** `transparent (write parser)` — parse_schlib_polyline hardcodes transparent: false. The struct field and writer.rs emit Transparent=T, so it
    round-trips on read but an AI cannot set it on write_schlib. Read json.get("transparent").
- **[round-trip | read]** `IsNotAccesible` — Altium's SchPolylineDto has ISNOTACCESIBLE and the golden POLYLINES/Data emits IsNotAccesible=T, but our Polyline struct has
    no is_not_accessible field, so parse_polyline never reads it and encode_polyline never writes it (unlike our Line/Arc/Bezier which DO carry this flag). Round-tripping
    an Altium polyline silently drops IsNotAccesible=T, changing the editor selectability of the primitive. Add the field (default true to match Altium) and emit/parse it
    like Line.
- **[feature-loss | read]** `AreaColor (fill colour)` — Altium polylines carry AREACOLOR (fill colour) — SchPolylineDto.AreaColor, populated by
    SchLibReader.CreatePolyline. Our Polyline struct has no fill_color field, so the parser ignores AREACOLOR and the writer never emits it. A filled/closed polyline's
    fill colour is EE-meaningful and is lost on read and round-trip.
- **[feature-loss | read]** `IsSolid (filled)` — Altium polylines have ISSOLID (SchPolylineDto.IsSolid) controlling whether the shape is filled. Our struct lacks a
    'filled' field, so parse_polyline never reads IsSolid and encode_polyline never writes it. Whether a polyline is filled is a real user-visible property, dropped on
    read and round-trip.
- **[feature-loss | read]** `OwnerPartDisplayMode` — Altium stores OWNERPARTDISPLAYMODE (SchPolylineDto.OwnerPartDisplayMode) selecting which display/alternate mode the
    primitive belongs to. Our struct has no field for it, so it is dropped on read. For multi-display-mode symbols a primitive bound to mode 1+ would collapse to mode 0
    on round-trip.
- **[round-trip | read]** `GraphicallyLocked` — Altium SchPolylineDto.GraphicallyLocked (GRAPHICALLYLOCKED) is read by the reference reader. Our Polyline struct lacks it,
    so it is not parsed or written; a locked polyline becomes unlocked on round-trip.
- **[round-trip | read]** `Disabled` — Altium SchPolylineDto.Disabled (DISABLED) is not represented in our struct; dropped on read and round-trip.
- **[round-trip | read]** `Dimmed` — Altium SchPolylineDto.Dimmed (DIMMED) is not represented in our struct; dropped on read and round-trip.
- **[round-trip | read]** `OwnerIndex` — Altium SchPolylineDto.OwnerIndex (OWNERINDEX) links the primitive to its parent record. Our parser ignores it and the writer
    never emits it. For SchLib single-component symbols this is typically -1/implicit, but it is part of Altium's record and is not preserved on round-trip.
- **[round-trip | read]** `X{n}_FRAC / Y{n}_FRAC vertex fractions` — AltiumSharp's SchPolylineDto carries per-vertex fractional coordinate parts (X1_FRAC, Y1_FRAC, ...)
    for sub-grid precision. parse_polyline reads only the integer x{i}/y{i} via coord(), discarding any _FRAC component; encode_polyline emits only integer X{n}/Y{n}.
    Sub-unit-positioned polyline vertices lose their fractional precision on read and round-trip (cosmetic-to-minor unless the source used fractional coords).

### SchLib Polygon (RECORD=7) (gaps: 9)

Audited SchLib Polygon (RECORD=7) coverage. Authoritative field list from AltiumSharp SchPolygonDto + CreatePolygon reader, sanity-checked against the committed golden
(scripts/samples/symbols.SchLib POLYGONS/Data) which confirms keys: RECORD, IsNotAccesible, IndexInSheet, OwnerPartId, LineWidth, AreaColor, IsSolid, LocationCount, X/Y
vertices, UniqueID. BIGGEST GAP (tool, feature-loss): the write_schlib tool does not support polygons at all — neither the input schema (tool_definitions.rs) nor the
handler (call_write_schlib in read_write.rs, which has no parse_schlib_polygon / symbol.add_polygon). An AI can READ polygons (struct serialized at read_write.rs:886) but
cannot WRITE or modify any polygon. So every otherwise round-trippable field (points, line_width, Color, AreaColor, IsSolid, owner_part_id, unique_id) is inaccessible for
authoring. READ/STRUCT GAPS (Altium stores, we drop on read and round-trip): Transparent (Polygon-specific omission — read+written for rectangles/ellipses but not
polygons), X{n}_FRAC/Y{n}_FRAC fractional vertex coords (points are plain i32), OwnerPartDisplayMode, Disabled, GraphicallyLocked, Dimmed, OwnerIndex. Plus
IsNotAccessible is hardcoded =T on write rather than preserved. Fields fully handled at the struct+reader+writer level (but still NOT tool-writable due to the missing
write schema): points/vertices, line_width/LineWidth, line_color/Color, fill_color/AreaColor, filled/IsSolid, owner_part_id/OwnerPartId, unique_id/UniqueID. These are
read-exposed and round-trip-safe; they are blocked only by the write-tool gap. Coverage is NOT complete. Key files: src/altium/schlib/primitives.rs (Polygon struct
520-544), src/altium/schlib/reader.rs (parse_polygon 588-633), src/altium/schlib/writer.rs (encode_polygon 431-462), src/mcp/tools/read_write.rs (read serialize line 886;
write handler 929-1204 has no polygon branch), src/mcp/tool_definitions.rs (write_schlib schema 331-471, no polygons key). Ground truth:
C:/tmp/AltiumSharp/src/OriginalCircuit.Altium/Serialization/Dto/Sch/SchPolygonDto.cs and Serialization/Readers/SchLibReader.cs CreatePolygon (1430-1472).

- **[feature-loss | tool]** `polygons (entire primitive) on write_schlib tool` — The write_schlib input schema in src/mcp/tool_definitions.rs (symbol item, ~lines
    351-460) exposes only pins, rectangles, lines, text, parameters, footprints — there is NO 'polygons' key. The handler call_write_schlib in src/mcp/tools/read_write.rs
    (~lines 1060-1158) likewise parses pins/rectangles/lines/polylines/arcs/ellipses/labels/text/parameters/footprints but never reads a 'polygons' array (no
    parse_schlib_polygon, no symbol.add_polygon). Net effect: an AI using this MCP cannot create or modify a polygon at all via the write tool. Every Polygon struct field
    that the writer CAN round-trip (points, line_width, line_color/Color, fill_color/AreaColor, filled/IsSolid, owner_part_id, unique_id) is unreachable because the tool
    layer accepts none of it. Read tool exposes polygons fine (symbol.polygons serialized at read_write.rs line 886), but it is read-only.
- **[feature-loss | read]** `Transparent` — Altium SchPolygonDto has TRANSPARENT (bool) and the reference reader maps it (SchLibReader.cs CreatePolygon ->
    .Transparent(dto.Transparent)). Our Polygon struct in src/altium/schlib/primitives.rs (lines 522-544) has NO transparent field; parse_polygon (reader.rs 588-633)
    never reads 'transparent'; encode_polygon (writer.rs 431-462) never emits it. Note this is a Polygon-specific omission: the same Transparent flag IS read+written for
    rectangles/ellipses/round_rects (reader.rs 419/568/657/798, writer.rs 414-416/521/549). EE-meaningful: controls whether the polygon fill is transparent vs opaque.
    Dropped on read and on round-trip.
- **[round-trip | read]** `X{n}_FRAC / Y{n}_FRAC (fractional vertex coordinates)` — Altium stores each vertex as integer X{n}/Y{n} plus optional X{n}_FRAC/Y{n}_FRAC
    sub-DXP fractional parts (SchPolygonDto X1Frac/Y1Frac/X2Frac... ; AltiumSharp combines via CoordFromDxp). Our points are Vec<(i32,i32)> (primitives.rs line 524);
    parse_polygon (reader.rs 600-607) reads only X{i}/Y{i} via coord() and ignores the _FRAC keys; encode_polygon (writer.rs 452-455) writes only integer X{i}/Y{i}, no
    _FRAC. A polygon authored in Altium with fractional vertex coords loses the fractional component on read and round-trip. (Our reader DOES handle _Frac for arc Radius,
    so the pattern exists but isn't applied to polygon vertices.)
- **[feature-loss | read]** `OwnerPartDisplayMode` — Altium SchPolygonDto OWNERPARTDISPLAYMODE; reference reader sets polygon.OwnerPartDisplayMode =
    dto.OwnerPartDisplayMode. Our struct/parser have no equivalent (primitives.rs Polygon has owner_part_id but not display mode; reader.rs parse_polygon never reads
    'ownerpartdisplaymode'; writer omits it). This selects which alternate display mode (Normal / Alternate 1...) the polygon belongs to — a real multi-representation
    feature. Dropped on read and round-trip.
- **[feature-loss | read]** `Disabled` — Altium SchPolygonDto DISABLED (bool); reference reader sets polygon.Disabled = dto.Disabled. Not present in our Polygon struct,
    not read by parse_polygon, not emitted by encode_polygon. EE-meaningful state flag (graphic disabled/excluded). Lost on read and round-trip.
- **[feature-loss | read]** `GraphicallyLocked` — Altium SchPolygonDto GRAPHICALLYLOCKED (bool); reference reader sets polygon.GraphicallyLocked = dto.GraphicallyLocked.
    Absent from our Polygon struct, parse_polygon, and encode_polygon. This is the per-primitive lock flag a user sets in Altium to prevent accidental moves; an AI
    editing the symbol would silently strip it. Lost on read and round-trip.
- **[round-trip | read]** `Dimmed` — Altium SchPolygonDto DIMMED (bool); reference reader sets polygon.Dimmed = dto.Dimmed. Not in our struct/parser/writer. Display
    state; lost on read and round-trip.
- **[round-trip | read]** `OwnerIndex` — Altium SchPolygonDto OWNERINDEX (int) links the polygon to its parent record; reference reader sets polygon.OwnerIndex =
    dto.OwnerIndex. Our Polygon struct has no owner_index; parse_polygon ignores it; encode_polygon (writer.rs 431-462) never emits OwnerIndex for the shape record (the
    writer only emits OwnerIndex for footprint/implementation records 45/46/48). For single-component SchLib symbols this is typically 0/implicit so usually harmless, but
    it is an Altium-stored field we neither read nor write — round-trip drops any non-default owner linkage.
- **[round-trip | read]** `IsNotAccessible` — Altium SchPolygonDto ISNOTACCESIBLE (bool, note Altium's misspelling); reference reader sets polygon.IsNotAccessible =
    dto.IsNotAccessible. Our parse_polygon never reads it, and encode_polygon HARD-CODES 'IsNotAccesible=T' (writer.rs line 436) regardless of the source value. So a
    polygon stored with IsNotAccesible=F (accessible) is read as nothing and rewritten as T — round-trip can flip the flag. Low severity since T is the near-universal
    value, but it is not faithfully preserved.

### SchArc (schematic arc, RECORD=12) (gaps: 12)

Audited the SchLib SchArc primitive across our model (struct Arc in src/altium/schlib/primitives.rs:548, reader parse_arc in src/altium/schlib/reader.rs:680, writer
encode_arc in src/altium/schlib/writer.rs:466), the MCP tools (read JSON via src/mcp/tools/read_write.rs:887 which serialises symbol.arcs directly; write parser
parse_schlib_arc in src/mcp/tools/parsing.rs:617; write_schlib input schema in src/mcp/tool_definitions.rs:346-464), against Altium ground truth (SchArcDto in
C:/tmp/AltiumSharp/.../Serialization/Dto/Sch/SchArcDto.cs and model SchArc.cs), sanity-checked against the golden scripts/samples/symbols.SchLib (ARCS/Data records
confirm IsNotAccesible/OwnerPartId/Location/Radius/LineWidth/EndAngle/UniqueID). Altium stores 20 parameters for RECORD=12. Our READ path (struct+reader+read JSON) covers
9 of them well (x, y, radius, start_angle, end_angle, line_width, color, fill_color/AreaColor, owner_part_id, is_not_accessible, unique_id). BIGGEST finding: the
write_schlib input SCHEMA does not declare an arcs array at all (only pins/rectangles/lines/text/parameters/footprints), so an AI literally cannot discover that arcs are
writable even though the code path parse_schlib_arc exists. Secondly, parse_schlib_arc hardcodes fill_color=0, is_not_accessible=true and unique_id=None, dropping three
fields on write even if they were supplied. Several Altium fields (OwnerIndex, OwnerPartDisplayMode, Location/Radius _FRAC fractional parts, GraphicallyLocked, Disabled,
Dimmed) have no struct field at all and are lost on read and round-trip. Reported one gap each below.

- **[feature-loss | tool]** `arcs (entire array)` — write_schlib's input_schema (src/mcp/tool_definitions.rs:351-461) lists only pins, rectangles, lines, text,
    parameters, footprints under a symbol — there is NO 'arcs' property (nor ellipses/polylines/polygons/beziers/round_rects). The code path exists (read_write.rs:1097
    reads sym_json['arcs'] and calls parse_schlib_arc), and the tool description mentions 'arcs', but because the schema never declares the key, an AI cannot discover or
    reliably emit arc objects. This makes every SchArc field unsettable in practice via the public tool contract. Add an 'arcs' array (x, y, radius, start_angle,
    end_angle, line_width, color, fill_color, is_not_accessible, owner_part_id) to the write_schlib symbol schema.
- **[feature-loss | tool]** `AreaColor (fill_color)` — The struct Arc.fill_color exists, the reader populates it from 'areacolor' (reader.rs:699), the writer emits it via
    nonzero("AreaColor", ...) (writer.rs:485), and read JSON exposes it. But the write parser parse_schlib_arc (parsing.rs:647) hardcodes fill_color: 0 and never reads
    json['fill_color']. So a filled arc round-tripped through the tools loses its fill colour: an AI cannot set the arc's area/fill colour on write.
- **[round-trip | tool]** `IsNotAccesible (is_not_accessible)` — Struct has is_not_accessible, reader reads it (reader.rs:712), writer emits it conditionally
    (writer.rs:468), read JSON exposes it. But parse_schlib_arc (parsing.rs:642) hardcodes is_not_accessible: true and ignores any value in the input JSON, so the flag
    cannot be set to false through the write tool. Altium emits this on real arcs (confirmed in golden symbols.SchLib), and it always defaults true, so impact is minor —
    but it is not settable, hence a tool gap.
- **[round-trip | tool]** `UniqueID (unique_id)` — Struct has unique_id, reader preserves it (reader.rs:719), writer emits it (writer.rs:486), read JSON exposes it. But
    parse_schlib_arc (parsing.rs:649) hardcodes unique_id: None, so when an AI writes a symbol via write_schlib the supplied/preserved UniqueID is discarded and a fresh
    one is generated — shape identity is not preserved across a tool-driven edit (the same #113 concern the struct comment guards against on the read path).
- **[round-trip | read]** `LOCATION.X_FRAC / LOCATION.Y_FRAC (centre fractional parts)` — Altium stores Location.X_FRAC and Location.Y_FRAC (SchArcDto:52,65) giving
    sub-unit (1/100000) centre precision. parse_arc (reader.rs:681-682) calls coord() which reads only the integer 'location.x'/'location.y' keys and ignores the _FRAC
    keys (unlike parse_elliptical_arc which DOES read radius_frac/secondaryradius_frac). The fractional offset is dropped on read, and the integer-only x:i32 struct field
    cannot represent it, so it is lost on write too. Centre is silently snapped to the integer grid.
- **[round-trip | read]** `RADIUS_FRAC (radius fractional part)` — Altium stores Radius_FRAC (SchArcDto:78) for sub-unit radius precision. parse_arc reads radius via
    coord(props,"radius") as an i32 (reader.rs:683) and never reads 'radius_frac'; the struct field radius:i32 cannot hold a fraction. Any fractional radius is truncated
    on read and not reproduced on write. (Contrast: the EllipticalArc model uses radius:f64 and reads radius_frac.)
- **[round-trip | read]** `OwnerIndex` — Altium stores OWNERINDEX (SchArcDto:15) linking the primitive to its parent record. The Arc struct has no owner_index field,
    parse_arc never reads it, and encode_arc never writes it (writer.rs:474 emits no OwnerIndex). For a top-level symbol primitive this is usually -1/implicit, but the
    value is not preserved — read-drop plus round-trip loss.
- **[feature-loss | read]** `OwnerPartDisplayMode` — Altium stores OWNERPARTDISPLAYMODE (SchArcDto:39), selecting which display/alternate mode (Normal vs Alternate
    graphics) the arc belongs to. No struct field, not read by parse_arc, not emitted by encode_arc. For multi-display-mode symbols this drops the arc's mode association,
    collapsing all arcs into the default mode on read and on write.
- **[feature-loss | read]** `GraphicallyLocked` — Altium stores GRAPHICALLYLOCKED (SchArcDto:114), the editor lock that prevents the arc from being moved/edited. No
    struct field, not read, not written. An AI cannot read whether an arc is locked nor set/preserve the lock; the flag is lost entirely on read and round-trip.
- **[round-trip | read]** `Disabled` — Altium stores DISABLED (SchArcDto:120). No struct field, not read by parse_arc, not emitted by encode_arc. The disabled state is
    dropped on read and cannot be reproduced on write.
- **[round-trip | read]** `Dimmed` — Altium stores DIMMED (SchArcDto:126), the dimmed-display flag. No struct field, not read, not written. Dropped on read and lost on
    round-trip.
- **[round-trip | read]** `IndexInSheet` — Altium stores INDEXINSHEET (SchArcDto:27). parse_arc never reads it into the struct (no field), so the original ordinal is
    dropped on read. The writer does emit IndexInSheet (writer.rs:474) but derives it from the primitive's position in the output sequence rather than preserving the
    source value, so the original index is not round-tripped. Low impact (positional indices are typically regenerated) but it is not a preserved field.

### Label (SchLib, RECORD=4) (gaps: 11)

Audited the SchLib Label primitive (RECORD=4) across our Rust model (src/altium/schlib/primitives.rs Label struct), reader (src/altium/schlib/reader.rs parse_label),
writer (src/altium/schlib/writer.rs encode_label), and MCP tools (read JSON in src/mcp/tools/read_write.rs:889 via direct serde of symbol.labels; write parser
src/mcp/tools/parsing.rs:692 parse_schlib_label fed from the "labels" array at read_write.rs:1115; write schema src/mcp/tool_definitions.rs) vs the authoritative
AltiumSharp DTO (C:/tmp/AltiumSharp/.../Dto/Sch/SchLabelDto.cs) and model (Models/Sch/SchLabel.cs), sanity-checked against the golden scripts/samples/symbols.SchLib via
olefile. Our struct covers x, y, text, font_id, color, justification, rotation, is_mirrored, is_hidden, owner_part_id, unique_id — all of which read + write +
read-tool-JSON fully (the read tool serializes symbol.labels directly so it exposes every struct field). Those are NOT gaps. AltiumSharp stores 20 serialized Label
parameters. Comparing reveals: (1) a TOOL gap — the write_schlib schema has NO "labels" key at all (only "text", which builds the separate Text primitive), so an AI
cannot discover it can write Labels; (2) unique_id is read/written/read-exposed but parse_schlib_label hardcodes it to None and no schema field accepts it, so a tool
round-trip loses identity; (3) eight Altium fields are absent from our struct entirely, dropped on both read and write: OwnerIndex, OwnerPartDisplayMode, AreaColor,
GraphicallyLocked, Disabled, Dimmed, IsNotAccessible (writer hardcodes IsNotAccesible=T), and Location X_FRAC/Y_FRAC fractional coordinates. IndexInSheet is also not
modelled (writer reuses the loop index, so an authored value cannot round-trip). The golden file only exercises the common subset (IsNotAccesible, OwnerPartId,
IndexInSheet, Location.X/Y, Justification, FontID, Orientation, Text, UniqueID), all of which our reader/writer handle; the missing fields come from the authoritative
AltiumSharp field list.

- **[feature-loss | tool]** `labels (write schema array)` — src/mcp/tool_definitions.rs defines NO 'labels' array in the write_schlib input schema. The write handler
    (read_write.rs:1115) DOES accept a 'labels' array routed to parse_schlib_label, but only the 'text' array (which builds the separate Text primitive) is documented. An
    AI reading the schema cannot discover that Labels can be written or which fields they take. Add a 'labels' array to the schema mirroring the struct fields (x, y,
    text, font_id, color, justification, rotation, is_mirrored, is_hidden, owner_part_id).
- **[round-trip | tool]** `unique_id` — unique_id is in the Label struct, set by parse_label (reader.rs:927), emitted by encode_label (writer.rs:646), and exposed on read
    (serde of symbol.labels). But the write parser parse_schlib_label (parsing.rs:746) hardcodes unique_id: None and ignores any incoming value, and no write-schema field
    accepts it. On a read-then-write round-trip via the MCP tools the AI cannot pass the original UniqueID back, so the writer generates a fresh one and the shape
    identity changes.
- **[round-trip | write]** `IsNotAccessible` — Altium stores ISNOTACCESIBLE (SchLabelDto.IsNotAccessible). Our writer hardcodes 'IsNotAccesible=T' (writer.rs:634)
    regardless of the actual value, and the struct has no field for it, so the reader drops it (parse_label never reads it). A Label with IsNotAccessible=F cannot be
    represented or round-tripped; it is always forced to T.
- **[feature-loss | read]** `AreaColor` — Altium stores AREACOLOR (background/fill colour, SchLabelDto.AreaColor / SchLabel.AreaColor). Our Label struct has no area_color
    field, parse_label never reads 'areacolor', and the writer never emits it. Data lost on read and not writable. EE-meaningful for label background fill.
- **[feature-loss | read]** `GraphicallyLocked` — Altium stores GRAPHICALLYLOCKED (SchLabelDto.GraphicallyLocked). Our struct has no field; parse_label never reads it and
    the writer never emits it. An AI cannot read or set whether a label is graphically locked.
- **[feature-loss | read]** `Disabled` — Altium stores DISABLED (SchLabelDto.Disabled). Not modelled in our Label struct, not read by parse_label, not emitted by the
    writer. Lost on read and unsettable.
- **[feature-loss | read]** `Dimmed` — Altium stores DIMMED (SchLabelDto.Dimmed, display dimming). Not modelled in our struct, not read, not written. Lost on read and
    unsettable.
- **[round-trip | read]** `OwnerIndex` — Altium stores OWNERINDEX (SchLabelDto.OwnerIndex) linking the primitive to its parent record in the hierarchy. Our Label struct
    has no owner_index field; parse_label never reads it and the writer never emits OwnerIndex. Hierarchy linkage is dropped on read and cannot round-trip.
- **[round-trip | read]** `OwnerPartDisplayMode` — Altium stores OWNERPARTDISPLAYMODE (SchLabelDto.OwnerPartDisplayMode), the display/alternate mode of the owning part.
    Not modelled, not read by parse_label, not emitted by the writer. Alternate-display-mode association is lost; the label always belongs to the default mode.
- **[round-trip | write]** `IndexInSheet` — Altium stores INDEXINSHEET (SchLabelDto.IndexInSheet). Our struct has no index_in_sheet field; parse_label drops the read
    value, and encode_label (writer.rs:634) substitutes the writer's own positional loop index rather than the authored/original value. An authored or preserved
    IndexInSheet cannot round-trip.
- **[round-trip | read]** `Location.X_FRAC / Location.Y_FRAC` — Altium stores fractional sub-coordinate parts LOCATION.X_FRAC and LOCATION.Y_FRAC
    (SchLabelDto.LocationXFrac/LocationYFrac) for sub-unit positioning. Our model stores only integer x/y (coord() reads the whole part); parse_label ignores the *_FRAC
    parameters and the writer never emits them. Sub-coordinate precision is dropped on read and write.

### SchLib Parameter (RECORD=41) (gaps: 20)

Audited the SchLib Parameter primitive across our Rust model (struct/reader/writer) and the two MCP tool surfaces, versus the AltiumSharp ground truth (SchParameterDto
record 41 + SchParameter model + CreateParameter mapping). OUR struct has 11 fields (name, value, x, y, font_id, color, hidden, read_only_state, param_type,
owner_part_id, unique_id). The reader populates all 11 and the writer round-trips all 11, so the core read/write loop is internally consistent for those fields. The read
tool (src/mcp/tools/read_write.rs:890) serialises the whole struct, so it exposes all 11 fields. The bug surface is the WRITE tool: parse_schlib_parameter
(src/mcp/tools/parsing.rs:535) and the create_symbols schema (src/mcp/tool_definitions.rs:430-447) accept only 8 fields and HARD-CODE read_only_state=0, param_type=0,
unique_id=None — so two already-supported struct fields are unsettable via the MCP, and a written parameter cannot preserve its UniqueID. Beyond that, Altium's record 41
carries ~25 EE/display properties our struct never models at all (Justification, Orientation, ShowName/HideName, IsMirrored, AreaColor, AutoPosition, IsConfigurable,
IsRule, IsSystemParameter, Description, TextHorz/VertAnchor, OwnerPartDisplayMode, GraphicallyLocked, plus the Designator-specific group and the %UTF8%Text flag). These
are read-dropped today. AltiumSharp DTO is the authoritative field list; the committed golden (symbols.SchLib) only exercises the minimal key subset, all of which our
model already covers, so nothing in the golden is currently corrupted — the gaps are latent against Altium-authored parameters that use the richer keys.

- **[feature-loss | tool]** `param_type / read_only_state (write tool input)` — The struct has param_type and read_only_state, the reader sets them, and the writer emits
    ParamType/ReadOnlyState. But the write tool can't set them: parse_schlib_parameter (src/mcp/tools/parsing.rs:563-564) hard-codes read_only_state:0 and param_type:0,
    and the create_symbols schema (src/mcp/tool_definitions.rs:435-444) lists no such properties. An AI authoring a symbol cannot mark a parameter read-only or set its
    type (String/Boolean/Integer/Float), even though the core fully supports it. manage_schlib_parameters (set/add) also omits both.
- **[round-trip | tool]** `unique_id (write tool input)` — unique_id is in the struct, read-preserved, and writer-emitted, and surfaced on the read tool JSON. But the
    write tool drops it: parse_schlib_parameter hard-codes unique_id:None (parsing.rs:566) and the schema has no unique_id property. So a read-modify-write through
    create_symbols/update regenerates a fresh UniqueID for every parameter instead of preserving the one the AI just read, breaking parameter identity continuity.
- **[feature-loss | read]** `Justification (JUSTIFICATION)` — Altium stores per-parameter text justification (0=BottomLeft..8=TopRight; SchParameterDto.Justification,
    mapped to SchParameter.Justification). Our Parameter struct has no justification field, so the reader drops it and the writer never emits it — the parameter's text
    anchor/alignment is lost on read and reset on write. (Our Label/Text primitives model this, but Parameter does not.)
- **[feature-loss | read]** `Orientation (ORIENTATION)` — Altium stores parameter text rotation as ORIENTATION (0/1/2/3 = 0/90/180/270 deg; SchParameterDto.Orientation).
    Our struct has no rotation/orientation field, so a rotated parameter label reads back with no rotation and writes flat. EE-meaningful for placed value/designator
    text.
- **[feature-loss | read]** `ShowName (SHOWNAME) / HideName (HIDENAME)` — Altium has two distinct visibility toggles for the name portion: SHOWNAME (show 'Name=Value')
    and HIDENAME (display only the value). Both are in the DTO and SchParameter (ShowName, HideName). Our struct models neither — only the whole-parameter 'hidden'
    (IsHidden). An AI cannot control whether the parameter's name label is displayed; the distinction is dropped on read and write.
- **[feature-loss | read]** `IsMirrored (ISMIRRORED)` — Altium stores a mirror flag for parameter text (SchParameterDto.IsMirrored). Our struct has no is_mirrored field,
    so a mirrored parameter is read as un-mirrored and written un-mirrored. (Our Label/Text primitives carry is_mirrored; Parameter omits it.)
- **[feature-loss | read]** `Description (DESCRIPTION)` — Altium stores a per-parameter Description string (SchParameterDto.Description -> SchParameter.Description). Our
    struct has no description field; it is dropped on read and never written. EE-meaningful free-text metadata an AI would want to surface/set.
- **[feature-loss | read]** `AreaColor (AREACOLOR)` — Altium stores the parameter's area/background colour (SchParameterDto.AreaColor). Our struct only has the foreground
    'color' field; AreaColor is dropped on read and never written.
- **[feature-loss | read]** `AutoPosition (AUTOPOSITION)` — Altium stores an auto-position mode (0=Manual, 1-4=auto anchor positions; SchParameterDto.AutoPosition). This
    drives how Altium auto-places the parameter label relative to the component. Our struct has no field for it, so it is dropped on read; on write Altium will treat the
    parameter as manually positioned (AutoPosition=0).
- **[feature-loss | read]** `IsConfigurable (ISCONFIGURABLE)` — Altium flags whether a parameter is variant-configurable (SchParameterDto.IsConfigurable). Dropped on
    read, never written. Relevant to variant management; an AI building variant-aware libraries cannot read or set it.
- **[feature-loss | read]** `IsRule (ISRULE)` — Altium flags a parameter as a design rule (SchParameterDto.IsRule); rule parameters carry PCB design-rule directives. Our
    struct has no field; dropped on read, never written.
- **[feature-loss | read]** `IsSystemParameter (ISSYSTEMPARAMETER)` — Altium flags system parameters (SchParameterDto.IsSystemParameter). Dropped on read, never written;
    an AI cannot distinguish a system parameter from a user one.
- **[feature-loss | read]** `TextHorzAnchor / TextVertAnchor (TEXTHORZANCHOR / TEXTVERTANCHOR)` — Altium stores horizontal and vertical text-anchor modes for the
    parameter (SchParameterDto.TextHorzAnchor / TextVertAnchor). These control text-box anchoring distinct from Justification. Both are dropped on read and never written.
- **[feature-loss | read]** `OwnerPartDisplayMode (OWNERPARTDISPLAYMODE)` — Altium scopes the parameter to a display mode (alternate symbol view) via OWNERPARTDISPLAYMODE
    (SchParameterDto.OwnerPartDisplayMode). Our struct only has owner_part_id, not the display-mode index, so a parameter bound to an alternate display mode is dropped to
    mode 0 on round-trip.
- **[feature-loss | read]** `GraphicallyLocked (GRAPHICALLYLOCKED)` — Altium stores a graphically-locked flag on the parameter (SchParameterDto.GraphicallyLocked). Our
    Parameter struct has no graphically_locked field (unlike our Pin), so a locked parameter reads unlocked and is written unlocked.
- **[round-trip | read]** `TextIsUtf8 (%UTF8%Text key prefix)` — When a parameter value contains non-Windows-1252 characters, Altium writes the key as '%UTF8%Text' and
    UTF-8-encodes the value (SchParameter.TextIsUtf8; CreateParameter decodes it). Our reader only looks up the plain 'text' key and decodes the whole record as
    Windows-1252, so a %UTF8%Text value is mis-decoded on read and re-emitted as a plain (Windows-1252) Text on write, corrupting non-Latin parameter values.
- **[round-trip | read]** `IsNotAccessible (ISNOTACCESIBLE)` — Altium tags the parameter record with ISNOTACCESIBLE (SchParameterDto.IsNotAccessible). Our Parameter
    struct has no field and our writer never emits it (unlike our rect/line/arc encoders, which always write IsNotAccesible=T). Read-dropped and write-omitted; a
    byte/selection-behaviour fidelity gap against Altium-authored parameters.
- **[round-trip | read]** `Disabled (DISABLED) / Dimmed (DIMMED)` — Altium stores Disabled and Dimmed display flags on the parameter (SchParameterDto.Disabled, Dimmed).
    Neither is modelled in our struct; both are dropped on read and never written.
- **[round-trip | read]** `Location.X_FRAC / Location.Y_FRAC (fractional coordinates)` — Altium stores sub-unit fractional coordinate parts (LOCATION.X_FRAC /
    LOCATION.Y_FRAC; SchParameterDto.LocationXFrac/LocationYFrac, combined via CoordFromDxp). Our reader only parses integer location.x/location.y and our writer only
    emits the integer part, so a parameter placed off the integer grid loses its fractional offset on round-trip. (Our EllipticalArc handles _Frac for radii, but
    Parameter does not for location.)
- **[round-trip | read]** `Designator-specific group: NameIsReadOnly, ValueIsReadOnly, AllowDatabaseSynchronize, AllowLibrarySynchronize, PhysicalDesignator,
    VariantOption` — The record-41 model also backs the Designator/Comment special parameters, which carry NAMEISREADONLY, VALUEISREADONLY, ALLOWDATABASESYNCHRONIZE,
    ALLOWLIBRARYSYNCHRONIZE, PHYSICALDESIGNATOR, VARIANTOPTION (all in SchParameterDto/SchParameter). Our parser handles the designator only as a bare RECORD=34 'text'
    string (reader.rs case 34) and drops these flags; for a generic parameter they are unmodelled. Dropped on read, never written.
