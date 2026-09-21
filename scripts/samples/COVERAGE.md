# Golden-fixture coverage map

Goal: the committed Altium-authored fixtures (`scripts/samples/symbols.SchLib`,
`scripts/samples/footprints.PcbLib`) should exercise **every** property each primitive can
carry, so the library-reading tests (`tests/samples_schlib.rs`, `tests/samples_pcblib.rs`)
reach 100% *real* read coverage.

## Why this matters — the circularity problem

Our test pyramid has three tiers, and only one of them is true ground truth for a
**populated** (non-default) field:

| Tier | Proves | Blind spot |
|------|--------|------------|
| Readability **oracle** (`test_altium_readability.py`) | our *default* output opens in pyaltiumlib | only exercises from-scratch defaults; says nothing about non-default field values |
| **Self-round-trip** (write→read→assert) | our writer and our reader agree | **circular** — a field read wrong *and* written wrong the same way still passes |
| **Golden fixtures** (Altium-authored) | we read a real Altium file correctly | only as good as the values the fixture actually contains |

Every "self-round-trip only" caveat traces to the golden fixtures not yet carrying the
field. The fix is always to enrich them (see the workflow below).

## How the fixtures are produced (fully automated, on this PC)

- `scripts/Generate-Samples.ps1` — launches Altium headless (RunScript CLI), runs the
  DelphiScript, copies the authored libraries into `scripts/samples/`.
- `scripts/Watch-AltiumDialog.ps1` — run alongside the generator. A compile error or
  a native crash opens a modal dialog that, headless, would just sit there until the
  7-minute timeout; this catches it, prints the offending identifier and kills
  Altium. It is what makes batching several unproven names into one run safe.
- `scripts/altium/generate/GenerateSamples.pas` — the **authoring logic** (editable here;
  DelphiScript). Header declares it *iterative by design*: generate → read back → add the next
  feature → regenerate, until coverage is complete.
- **Standing workflow:** when a read test needs a feature the goldens don't carry, extend the
  `.pas`, run `python scripts/altium/generate/preflight_names.py` and reduce it to **one**
  unproven interface (one bad identifier aborts the whole script compile), kill any stale
  `X2` process, regenerate locally, commit the binaries, then write **exact** (non-guarded)
  assertions against the authored values. No tolerant or skipping tests.
- **Documented negatives:** when Altium does not persist an authored property, record the
  negative in the `.pas` next to the helper so it is not retried blindly, and mark the row
  below 🚫. The evidence for each lives once, in
  [Verified negatives](#verified-negatives--do-not-retry) below.
- **Cross-check from Altium's side:** `scripts/Verify-Libraries.ps1 -Files <goldens>
  -Expect scripts/samples/golden_expectations.json` makes a real Altium assert the
  component names and per-component primitive counts this reader finds in the goldens —
  the inverse of the fixture direction, so a primitive Altium quietly drops cannot hide
  behind "opened". `tests/golden_expectations.rs` keeps the expectations file current.

## Coverage map

Legend: ✅ authored + asserted · ❌ not exercised (self-round-trip only) · 🚫 documented
negative — the evidence for each lives once, in [Verified negatives](#verified-negatives--do-not-retry).
library (see below — no fixture is possible, and none is needed).

### Structurally absent: the net index

`net_index` (common header @3) indexes a **board's** net list. A `PcbLib` has no net
table — the golden's 24 top-level OLE entries are `FileHeader`, `FileVersionInfo`,
`Library` and the footprint storages, with nothing for the index to point at — so every
primitive in every library reads the `0xFFFF` "no net" sentinel. All 33 pads, tracks and
vias in the golden do.

No golden can therefore exercise a non-sentinel value, and none needs to: the reader does
not branch on it beyond the short-header fallback, which a well-formed file never takes.
Both paths are covered by `common_indices_decode_values_sentinels_and_short_headers` (real
values, the sentinel, and every truncation point) and by `binary_roundtrip_common_indices`
for the board-context encode/decode. The rows below are marked 🔒 rather than ❌ so the
distinction stays visible: this is not an authoring gap waiting on an Altium run.

### PcbLib (`footprints.PcbLib`)

| Primitive | Exercised today | Not exercised (❌) |
|-----------|-----------------|--------------------|
| Pad | shape (round/rect/oct/rrect), TH holes (round/square/slot), local stack, rotation, negative/far coords, ✅ rotated unplated slot (`PRIMPROPS` S1: a 20×40 mil slot at 30°, `Plated=False`), ✅ manual paste/solder-mask expansion (`PADMASK`, authored via the pad cache); ✅ mask expansion from the hole edge (`PADMASK` pad 3, main-block bool @125 — offset derived by byte-diffing against pad 1), ✅ locked + keepout flags, ✅ drill tolerances (`LOCKFLAGS_PCB`); ✅ jumper group (`LOCKFLAGS_PCB` pads 7-8 share id 4, `samples_pcblib_jumper_group`); ✅ fabrication test points top/bottom (`LOCKFLAGS_PCB` pads 5-6, `samples_pcblib_testpoint_flags` — Altium also locks a test-point pad, so both read LOCKED); ✅ Mechanical 20 (`MECH20`: byte 72 + V7 id `0x01020014`); ✅ per-pad polygon-connect override — style, air gap, conductor width, 2/4/Auto conductors, 45°/90° (hand-authored `manual/thermal_relief.PcbLib`, `samples_manual_pad_polygon_connect`) | 🚫 assembly test points; 🚫 DrillType; 🚫 fabrication flags; 🚫 corner-radius `CRPercentage` (crashes on a fresh Simple pad — needs correct pad-stack init first); 🚫 **FINAL** thermal-relief / power-plane setters (`PowerPlaneConnectStyle` / `ReliefConductorWidth` / `ReliefEntries` / `ReliefAirGap` / `PowerPlaneClearance` crash AD24's ScriptingSystem.DLL with a native access violation in **every** scripted sequence tried — pre- and post-registration, with and without the `GetState_Cache` block; batch 4a + 4b bisects. `PAD_THERMAL` cannot be authored by script in AD24 and stays disabled in the `.pas`) |
| Via | simple TH, two pad/hole sizes; ✅ mask-expansion cache state + 4 mil template expansion (`samples_pcblib_via_mask_state_is_altium_factory_default` — an Altium via carries byte @66 = 0, `eCacheInvalid`); ✅ manual solder + paste mask expansion (`PRIMPROPS`, set through `TPadCache`) | thermal-relief, power-plane, GUID; 🔒 mask-from-hole-edge (@258) + drill-pair type (@312) — modelled and round-tripped, but AD24 exposes no setter for either, so no script can author a non-default value; 🔒 net index; 🚫 tenting flags |
| Track | silk box + copper track, two widths, two layers; ✅ multi-layer spread (`MULTILAYER`: six tracks on Mechanical 2 / Mid-Layer 5 / Drill Guide / Drill Drawing / Internal Plane 1 / Keep-Out — real golden coverage for `layer_from_id`'s exotic arms; ID 58 reads as the documented `TopAssembly` alias, `samples_pcblib_multilayer`); ✅ locked + keepout (`LOCKFLAGS_PCB`); ✅ Mechanical 20 (`MECH20`: byte 72 + V7 id `0x01020014`) | 🔒 net index |
| Arc | full circle + quarter arc; ✅ locked + keepout (`LOCKFLAGS_PCB`); ✅ Mechanical 20 (`MECH20`: byte 72 + V7 id `0x01020014`) | fill/area colour; 🔒 net index |
| Region | copper box + mechanical box; ✅ board-cutout representation (`ISBOARDCUTOUT=TRUE` + `KEEPOUT=TRUE`, relocated to the keep-out layer — `samples_pcblib_region_cutout`); ✅ every `TRegionKind` (`PRIMPROPS`: KIND=2 NamedRegion, KIND=4 Cavity, KIND=1 Cutout — a board cutout has no KIND of its own); ✅ name + union index; ✅ Mechanical 20 (`MECH20`: byte 72 + `V7_LAYER=MECHANICAL20`); ✅ a hole contour (`manual/region_hole.PcbLib`, scripted through `GeometricPolygon.AddContourIsHole` — `samples_manual_region_hole_reads_exactly`) | 🔒 net (a `PcbLib` stores none: `NetProbe.pas`); cavity/subpoly params; 🚫 arc resolution (not on `IPCB_Region`) |
| Fill | axis-aligned + 45°-rotated copper; ✅ locked + keepout (`LOCKFLAGS_PCB`); ✅ Mechanical 20 (`MECH20`: byte 72 + V7 id `0x01020014`) | 🔒 net index |
| Text | stroke text, Win-1252 chars, vertical (90°); ✅ TrueType `font_name`='Arial' + bold + italic + mirror (`TEXT_STYLE`); ✅ kind=BarCode (`TEXT_SPECIAL` 'BC128'); ✅ inverted (knockout) text + inverted-rect descriptor (`TEXT_SPECIAL` 'INV': `is_inverted`, `use_inverted_rectangle`, `inverted_border`=10 mil, explicit `InvRectWidth`/`InvRectHeight` 120×70 mil — authored, because AD computes the size lazily from the rendered extent and a headless save can catch it at 0); ✅ barcode sizing block (`TEXT_SPECIAL` 'BC2': full width/height, X/Y margins, symbology and UTF-16LE font name); ✅ barcode inverted + show-text (`BC3`/`BC4`, each differing from `BC2` in exactly one field, pinning @159 and @225); ✅ stroke-font variants + 12 mil stroke width (`PRIMPROPS`: FontID 2 Sans Serif, 3 Serif — the reader only surfaces a font above id 1) ; ✅ stroke-font variants + non-default stroke width (`PRIMPROPS`: FontID 2 Sans Serif and 3 Serif, 12 mil width — the reader only surfaces a stroke font above id 1); ✅ Mechanical 20 (`MECH20`); ✅ WideStrings-authoritative text (`TEXT_WIDE_ONLY`: U+0094 is `?` in the Data stream and 148 in `ENCODEDTEXT`) | 🚫 justification (`TextJustification` is not on `IPCB_Text`); 🚫 barcode MinWidth and RenderMode; 🚫 text beyond U+00FF (see backlog) |
| ComponentBody | one extruded box (Mechanical); ✅ embedded STEP model (`EMBSTEP`: `MODELID`/`MODEL.CHECKSUM`/`MODEL.NAME` on the body + zlib model stream in `/Library/Models/0`, decompressed `ISO-10303-21` bytes exact-asserted — `samples_pcblib_embstep`); ✅ standoff + cavity height + 3D colour + opacity (`PRIMPROPS` — `cavity_height` was not modelled at all until this fixture exposed it); ✅ referenced (non-embedded) STEP model (`STEP_REF`: `MODEL.EMBED=FALSE` with a `MODELID` and a `/Library/Models` entry all the same — the script-authored form; the UI-authored form has neither); ✅ Mechanical 20 (`MECH20`); ✅ raw-outline precision (`BODYPREC`: vertices off-grid by ±1..9 raw internal units — AD24 keeps the exact units through save) | model 2D location/rotation (no scripting setter found: `Rotation2D`/`Model2D*` are absent from the identifier table; the UI-authored evidence lives in `manual/identifier.PcbLib`) |

### SchLib (`symbols.SchLib`)

| Primitive | Exercised today | Not exercised (❌) |
|-----------|-----------------|--------------------|
| Pin | electrical types (all 8), orientations (0/90/180/270), name/designator visibility, edge decorations, dual-part `owner_part_id`; ✅ PinFrac off-grid coords (`FRACPINS`), ✅ PinSymbolLineWidth (`Symbol_LineWidth=eLarge`); ✅ swap-id tail (`SWAPPIN`: `SwapId_Pin`→`swap_id_group`='A', `SwapId_Part`→`part_and_sequence`='1', `DefaultValue`→`default_value`='3V3'); ✅ owner_part_display_mode=1 (`DISPMODE` pin 2 — the pin-record byte); ✅ graphically_locked (`LOCKFLAGS` pin — flag bit 0x40 of the binary record) | — |
| Line | plain segments; ✅ line_style dashed + dotted (`SHAPESTYLE`); ✅ non-default colour (`SHAPECOLOR`); ✅ display flags (`LOCKFLAGS2`); ✅ `_Frac` coords (`FRACSHAPES2`) | 🔒 is_not_accessible=false |
| Arc | plain arcs; ✅ `_Frac` coords (`FRACSHAPES`: centre (0.05, 0.05), radius 4.05 — AD24 omits the zero integer keys and stores frac-only); ✅ non-default colour + non-zero `StartAngle` (`SHAPECOLOR`); ✅ display flags (`LOCKFLAGS2`) | 🚫 fill/area colour (an `ISch_Arc` has no fill — `Arc.IsSolid` does not compile); 🔒 is_not_accessible=false |
| Rectangle | plain rects; ✅ transparent (`SHAPESTYLE`), ✅ GraphicallyLocked (`LOCKFLAGS`); ✅ `_Frac` coords incl. negatives (`FRACSHAPES`: (-5.45, -2.45)–(5.55, 2.55)); ✅ non-default border + fill colour (`SHAPECOLOR`); ✅ line_style dashed (`SHAPESTYLE` — persists as `LineStyleExt` between `Corner.Y` and `LineWidth`, unlike the round-rect, which drops the key) | 🚫 Disabled/Dimmed (authored but not persisted by AD24) |
| RoundRect | plain rounded rects; ✅ non-default border colour (`SHAPECOLOR`); ✅ display flags (`LOCKFLAGS2`); ✅ `_Frac` coords (`FRACSHAPES2`) | 🚫 line_style and 🚫 transparent — both accepted by AD24 and neither written to a library round-rect |
| Ellipse | plain ellipses; ✅ transparent; ✅ non-default border colour (`SHAPECOLOR`); ✅ display flags (`LOCKFLAGS2`); ✅ `_Frac` coords (`FRACSHAPES2`) | — |
| Polyline | plain polylines; ✅ non-default colour (`SHAPECOLOR`); ✅ line_style + start/end shapes + shape size (`SHAPESTYLE2`: dashed, arrow → solid arrow, eLarge — all four persist); ✅ display flags (`LOCKFLAGS2`); ✅ `_Frac` coords (`FRACSHAPES2`) | 🚫 fill/transparency (AreaColor/IsSolid/Transparent compile on `ISch_Polyline` and none is written) |
| Polygon | plain polygons; ✅ transparent (`SHAPESTYLE` triangle); ✅ non-default border colour (`SHAPECOLOR`); ✅ display flags (`LOCKFLAGS2`); ✅ `_Frac` coords (`FRACSHAPES2`) | 🔒 is_not_accessible=false (line_style: N/A — `ISch_Polygon` has no LineStyle in AD24); 🔒 is_not_accessible=false |
| Pie | ✅ authored (`PIESYM`: 30–210°, radius 5 units, yellow fill, exact-asserted); ✅ non-default border + fill colour (`SHAPECOLOR`); ✅ display flags (`LOCKFLAGS2`); ✅ `_Frac` coords (`FRACSHAPES2`) | 🚫 transparent (`ISch_Pie` has none — `Pie.Transparent` does not compile) |
| Image | ✅ authored (`IMAGESYM`: bounding box, `logo.bmp`, KeepAspect, non-embedded); ✅ embedded image bytes in the `/Storage` stream (`EMBIMGSYM`, exact-asserted against the committed `embed.bmp`); ✅ display flags (`LOCKFLAGS2`) | 🚫 show_border (not on `ISch_Image` — does not compile) |
| Bezier | ✅ authored (`BEZIERSYM`, four control points exact-asserted); ✅ non-default colour + eMedium width (`SHAPECOLOR`); ✅ display flags (`LOCKFLAGS2`); ✅ `_Frac` coords (`FRACSHAPES2`) | — |
| Label | plain labels; ✅ justification variants + rotation (`JUSTIFY`); ✅ non-default colour (`SHAPECOLOR`); ✅ mirrored (`SHAPESTYLE2`); ✅ display flags (`LOCKFLAGS2`); ✅ `_Frac` coords (`FRACSHAPES2`) | — |
| Parameter | Value etc.; ✅ justification + orientation (`JUSTIFY`: `Justification=8` on Value, `Justification=4` + `Orientation=1` on the hidden Tol); ✅ autoposition + justification from the hand-authored `manual/parameters.SchLib`; ✅ show_name + read_only_state + is_mirrored + param_type (`SHAPESTYLE2` — `is_mirrored` was not modelled at all until this fixture exposed it) | 🚫 is_rule / is_system_parameter / is_configurable / text anchors — read-only or never written into a library |
| EllipticalArc | ✅ authored (`ELLARC`: radius 5 / secondary 3, 0–270°); ✅ `_Frac` on centre and both radii; ✅ GraphicallyLocked (`GraphicallyLocked=T` after `OwnerPartId`, as on every graphic) | 🚫 Disabled/Dimmed (not persisted on library shapes) |
| FootprintModel | ✅ the RECORD=44/45/46/48 chain (`IMPLCHAIN`: current link with a datafile — `DatafileCount=1`, `ModelDatafile0`, entity, kind, `IsCurrent=T` — a name-only link with none of them and no `Description`, and a described non-current one) | 🚫 `IntegratedModel`/`DatabaseModel` (settable, not persisted from a script — hand-authored evidence only) |
| IeeeSymbol | ✅ authored (`IEEESYM`: a dot, a mirrored rotated clock, a locked coloured active-low input at scale 20 — `Symbol`, `ScaleFactor`, `Orientation`, `Mirror`, `Color`, `GraphicallyLocked`, no `UniqueID`) | 🚫 Disabled/Dimmed (not persisted on library shapes) |

> **Five i18n symbols of the generated golden are internally inconsistent** (`_JV`, `_BN`,
> `_CR`, `_IU`, `_SB`): the script engine mis-decodes exactly those source sequences, and
> every scripted repair is a verified negative (see `DOCUMENTED NEGATIVE` in
> `GenerateSamples.pas`). The hand-authored `scripts/samples/manual/i18n5.SchLib` (AD24 UI)
> carries all five consistently and is the ground truth for the UI-authoring convention
> (plain keys are ANSI `?` husks; real names in raw-UTF-8 `%UTF8%` twins; pin names only in
> `PinWideText`). The damaged copies stay in the generated golden, excused by suffix
> (`FIXTURE_INCONSISTENT` in `tests/golden_fidelity.rs`); never open-and-save that golden in
> AD (see `scripts/samples/README.md`).

`manual/pipe.SchLib` + `manual/pipe.PcbLib` (AD24, scripted 2026-08-30 through the API) pin
what Altium does with a `|` in text: the schematic editor stores it as `¦` (U+00A6); the PCB
editor writes it raw and reads the description back cut at it (`Verify-Libraries.ps1`
reports a length of 1 for `A|B=C`). The writers refuse a `|` on that evidence.

### Cross-cutting (both formats)

- **Universal display/lock flags** — `GraphicallyLocked` is golden-covered on Rectangle
  (`LOCKFLAGS`); `Disabled`/`Dimmed` are 🚫 documented AD24 negatives (not persisted on
  library shapes); `OwnerPartDisplayMode` at a non-default value is now ✅ golden-covered
  (`DISPMODE`: a `DisplayModeCount=2` symbol whose mode-1 rectangle AND mode-1 pin carry
  `OwnerPartDisplayMode=1` — `samples_schlib_dispmode`, pin byte included).
- **`unique_id`** — present in fixtures, so identity read is covered; but per-primitive GUID
  streams for populated cases are thin.
- **Fractional coordinates** — the Pin `_Frac` path is golden-covered via the `PinFrac` aux
  stream (`FRACPINS`); the text-record `*_Frac` key path on graphic shapes is ✅
  golden-covered too (`FRACSHAPES`, `FRACSHAPES2`). The goldens pin the convention:
  AD24 stores negative off-grid coordinates as **truncation toward zero with a SIGNED
  `_Frac`** (`Location.X=-5|Location.X_Frac=-45000` = −5.45) and **omits a zero integer
  key** when only the fraction is non-zero (`Location.X_Frac=5000` with no `Location.X`);
  reader and writer follow it (see `docs/SCHLIB_FORMAT.md` § Fractional coordinates).

## Remaining enrichment backlog

Each batch: extend the `.pas` → run `preflight_names.py` → regenerate locally → commit
binaries → exact assertions.

An unproven `(interface, property)` pair is only a *compile* risk, and a failure names the
identifier in a modal dialog, so several may go in one run provided that dialog is read
rather than waited out — otherwise keep it to one unproven interface, or a timeout will
not say which name was at fault.

**SchLib:** `IntegratedModel`/`DatabaseModel` on a footprint link: settable by script
(`ISch_Implementation`), but AD24 does not persist them from a script — the UI-authored form
(`IntegratedModel=T|DatabaseModel=T`) is known from a hand-authored library and replayed
verbatim, not from a golden.

**PcbLib:** region net and the cavity/subpoly params. A region *hole* is covered: after
outlining a region the proven way, `Rgn.GeometricPolygon.AddContourIsHole(Contour, True)`
adds it (`scripts/altium/probe/RegionHoleProbe.pas`, 2026-09-21), and
`manual/region_hole.PcbLib` pins the result. The region `NET` is 🔒 structurally absent,
like the net index: `scripts/altium/probe/NetProbe.pas` (2026-09-21) creates a net, adds
it to the library board and assigns it to a copper region — Altium accepts all three in
memory and reads the net back as `GND` — yet the saved region carries no `NET` key and its
common-header net index is 65535. A `PcbLib` has no net table to persist it in.
A **text beyond U+00FF** (Ω, CJK) is ✅ covered by `manual/wide.PcbLib`: a string literal
reaches Altium as its UTF-8 bytes widened through the machine's ANSI page and `Chr(N)`
truncates modulo 256, but a *character* literal (`#937`, `#55362#57271`) keeps its UTF-16
unit (`scripts/altium/probe/WideProbe.pas`, 2026-09-21), which also authors body
identifiers beyond the BMP through `SetState_Identifier`. A **via block
longer than the 321-byte template** (an older Altium's 351-byte vias) cannot come from
AD24 at all — hand-authored evidence only.
Pad thermal relief is 🚫 **FINAL** on the scripting side (native crash on a fresh library
pad in every sequence tried — see the Pad row), but the AD24 UI authors it: its Pad Stack →
Thermal Relief box writes the per-pad polygon-connect override, now pinned by the
hand-authored `manual/thermal_relief.PcbLib`. The power-plane connection bytes (@67-85)
stay without an Altium-authored non-default value: that box does not touch them.

## What "supported" means

A property counts as supported when all three hold — it is a field on the primitive struct,
`src/altium/{pcblib,schlib}/` parses and writes it, and an entry in
`src/mcp/tool_definitions.rs` reaches it — and as *proven* when a golden fixture exercises
it. Name matching between Altium's property names and struct fields only narrows the
candidates (`IsKeepout` and `IsTentingTop` live in `PcbFlags`, not in fields of their own),
so a claimed gap is confirmed by reading the parser.

To locate an unknown byte offset, author two primitives differing in exactly one field and
diff their record blocks — the offset carrying the authored value is the field. That is how
the pad's mask-from-hole-edge flag at `@125` and the six barcode sizing offsets were found.

## Verified negatives — do not retry

Properties AD24 accepts without error but does not persist in a library, each confirmed by
authoring it and reading the saved bytes back.

| Property | Evidence |
|----------|----------|
| Via tenting (`IsTenting_Top`/`_Bottom`) | authors fine; saved flag word is empty |
| Assembly test points (`IsAssyTestPoint_Top`/`_Bottom`) | flag word comes back a plain `0x000C` |
| `TearDrop`, `UserRouted` | same `0x000C`, identical to an untouched pad |
| `DrillType` | saved pad is byte-identical to a plain TH pad apart from coordinates |
| `IsBackDrill`, `IsCounterHole`, `IsPreRoute` | derived board state; no per-pad property |
| Barcode `MinWidth` | `@153` reads 39604/88235 against an authored 5 mil — Altium computes it |
| Barcode `RenderMode` | moves no byte; `@115` is a creation-order ordinal, not the property |
| PCB text justification | `TextJustification` does not exist on `IPCB_Text` |
| Net index (any primitive) | a `PcbLib` has no net table, so it is always `0xFFFF` |
| SchLib `IsRule` | AD24 marks a rule by `Name=Rule` plus a `RULEKIND=…` payload in `Text`, not by a flag |
| SchLib `IsSystemParameter` | absent even on `Comment`; not written into a library |
| SchLib `IsConfigurable` | absent from every parameter record in an authored library |
| SchLib `TextHorzAnchor` / `TextVertAnchor` | absent from every parameter record in an authored library |
| SchLib `IsNotAccesible` = false (Altium's spelling) | every graphic record in a library carries `=T`; no library case omits it |
| SchLib arc fill (`IsSolid` / `AreaColor`) | `Arc.IsSolid` does not compile — an `ISch_Arc` is a stroked shape with no fill |
| SchLib pie `Transparent` | `Pie.Transparent` does not compile — real on rectangle/round-rect/ellipse/polygon, absent from `ISch_Pie` |
| SchLib round-rect `LineStyle` | accepted without error, but the saved `RECORD=10` carries no `LineStyle` key — `ISch_Line` and `ISch_Polyline` both persist it |
| SchLib text-frame `Orientation` | not on `ISch_TextFrame` — `Frm.Orientation` does not compile, though it is real on label/parameter/pin |
| SchLib image `ShowBorder` | not on `ISch_Image` — `Img.ShowBorder` does not compile, though it is real on `ISch_TextFrame` |
| SchLib polyline fill (`AreaColor`/`IsSolid`/`Transparent`) | all three compile on `ISch_Polyline` and none is written; the rectangle and polygon records do persist theirs |
| SchLib parameter area colour | there is no such property: `parse_parameter` reads no colour-fill key and no authored parameter record carries one |
| SchLib text-frame `Transparent` | accepted, then not written — the saved `RECORD=28` has no `Transparent` key |
| PcbLib region `ArcResolution` | not on `IPCB_Region` — `Rgn.ArcResolution` does not compile, though the name is real elsewhere in the identifier table |
| PcbLib via mask expansion via DIRECT setters | `Via.SolderMaskExpansion*` / `.PasteMaskExpansion*` compile, then crash AD24 with a native access violation in `ScriptingSystem.DLL`. Set them through `TPadCache` (`GetState_Cache` → `SetState_Cache`) instead, which works |

## Deciding whether a property is settable at all

Two separate questions, and answering only the first is what makes a run fail.

**Does the name exist?** The DelphiScript engine's identifier table lives in
**`ScriptingSystem.dll`** as **UTF-16LE** strings — not in `Advpcb.dll` / `AdvSch.dll`, and
not in the `Altium.*.dll` .NET assemblies. The test is presence, and only presence:
absence means the name does not resolve at all (`TextJustification`, `eJustify_CenterCenter`),
while a hit means it is worth trying. Enum literals are in the same table, so check those
too — `eJustify_Center` is there, `eJustify_CenterCenter` is not.

Do **not** read anything more into the `SetState_` / `GetState_` prefixes. They are absent
for plenty of settable properties: `StandoffHeight`, `BodyColor3D` and `BodyOpacity3D` have
neither and all three set and persist. A missing `Set*` method proves nothing either —
`SolderMaskExpansionFromHoleEdge` and `BarCodeKind` both lack one and both set fine.

```python
import re
b = open(r'C:\Program Files\Altium\AD24\System\ScriptingSystem.dll', 'rb').read()
ids = set()
for m in re.finditer(rb'(?:[\x20-\x7e]\x00){3,}', b):
    ids |= set(re.findall(r'[A-Za-z_][A-Za-z0-9_]{2,}', m.group().decode('utf-16-le')))
'SetState_CavityHeight' in ids     # True -> settable
```

**Does this interface carry it?** The table is global, so a hit only says the name is real
*somewhere*. Resolution is per-interface and happens at compile time: `IsSolid` is in the
table and genuine on a rectangle, yet `Arc.IsSolid` still fails with `Undeclared
identifier: IsSolid`. Only a run settles this, so `scripts/altium/generate/preflight_names.py`
lists every `(interface, property)` pair with no precedent in the committed `.pas` — keep
it to **one unproven interface per run**, or a failure will not say which name was at
fault. An unresolved identifier aborts the whole compile and takes every other footprint
in that run with it; `try/except` cannot help, because nothing has run yet.

A third failure mode is worse than either: some properties compile and then take AD24 down
with a native access violation. Both known cases — pad thermal relief and via mask
expansion — sit on the pad cache, and the via one is avoidable by going through
`TPadCache` rather than the direct setters. Watch the run for the crash dialog instead of
waiting out the timeout, or a single bad line costs seven minutes and a hung Altium.
