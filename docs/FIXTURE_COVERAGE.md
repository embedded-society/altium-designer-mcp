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
- `scripts/altium/generate/GenerateSamples.pas` — the **authoring logic** (editable here;
  DelphiScript). Header declares it *iterative by design*: generate → read back → add the next
  feature → regenerate, until coverage is complete.
- **Standing workflow:** when a read test needs a feature the goldens don't carry, extend the
  `.pas` (verified AD24 API names only — one bad identifier aborts the whole script compile),
  kill any stale `X2` process, regenerate locally, commit the binaries, then write **exact**
  (non-guarded) assertions against the authored values. No tolerant or skipping tests.
- **Documented negatives:** when Altium does not persist an authored property, record the
  negative in the `.pas` next to the helper (and in the tables below) so it is not retried
  blindly. Known AD24 negatives: `Disabled`/`Dimmed` on a library rectangle,
  `ISch_RoundRectangle.Transparent`, `Pad.CRPercentage[...]` on a fresh Simple pad (native
  crash), and the pad thermal-relief / power-plane setters on a fresh library pad (native
  crash — see the Pad row).

## Coverage map

Legend: ✅ authored + asserted · ❌ not exercised (self-round-trip only) · 🚫 documented
negative (Altium does not persist it — do not retry).

### PcbLib (`footprints.PcbLib`)

| Primitive | Exercised today | Not exercised (❌) |
|-----------|-----------------|--------------------|
| Pad | shape (round/rect/oct/rrect), TH holes (round/square/slot), local stack, rotation, negative/far coords | paste/solder-mask expansion, testpoint flags, slot geometry on SMD, drill tolerances, jumper id, locked/keepout flags; 🚫 corner-radius `CRPercentage` (crashes on a fresh Simple pad — needs correct pad-stack init first); 🚫 thermal-relief / power-plane setters (`PowerPlaneConnectStyle` / `ReliefConductorWidth` / `ReliefEntries` / `ReliefAirGap` / `PowerPlaneClearance` crash AD24's scripting engine with a native access violation on a fresh library pad, with or without `GetState_Cache` — batch 4a bisect; `PAD_THERMAL` stays disabled in the `.pas`) |
| Via | simple TH, two pad/hole sizes | thermal-relief, power-plane, tenting flags, net index, paste-mask expansion, GUID |
| Track | silk box + copper track, two widths, two layers | locked/keepout flags, net index |
| Arc | full circle + quarter arc | fill/area colour, locked/keepout, net index |
| Region | copper box + mechanical box; ✅ board-cutout representation (`ISBOARDCUTOUT=TRUE` + `KEEPOUT=TRUE`, relocated to the keep-out layer — `samples_pcblib_region_cutout`) | named region, net, arc-resolution/cavity/subpoly/union params |
| Fill | axis-aligned + 45°-rotated copper | locked/keepout, net index |
| Text | stroke text, Win-1252 chars, vertical (90°); ✅ TrueType `font_name`='Arial' + bold + italic + mirror (`TEXT_STYLE`); ✅ kind=BarCode (`TEXT_SPECIAL` 'BC128'); ✅ inverted (knockout) text + inverted-rect descriptor (`TEXT_SPECIAL` 'INV': `is_inverted`, `use_inverted_rectangle`, `inverted_border`=10 mil, auto-computed rect width/height exact-asserted) | justification, stroke_font variants, barcode sizing block (not modelled), char_set, union_index, net/component index, flags |
| ComponentBody | one extruded box (Mechanical) | embedded STEP model, cavity height, model 2D location/rotation, non-default colour/opacity, raw-outline precision |

### SchLib (`symbols.SchLib`)

| Primitive | Exercised today | Not exercised (❌) |
|-----------|-----------------|--------------------|
| Pin | electrical types (all 8), orientations (0/90/180/270), name/designator visibility, edge decorations, dual-part `owner_part_id`; ✅ PinFrac off-grid coords (`FRACPINS`), ✅ PinSymbolLineWidth (`Symbol_LineWidth=eLarge`); ✅ swap-id tail (`SWAPPIN`: `SwapId_Pin`→`swap_id_group`='A', `SwapId_Part`→`part_and_sequence`='1', `DefaultValue`→`default_value`='3V3') | owner_part_display_mode (non-default), graphically_locked |
| Line | plain segments; ✅ line_style dashed + dotted (`SHAPESTYLE`) | is_not_accessible=false, display flags |
| Arc | plain arcs; ✅ `_Frac` coords (`FRACSHAPES`: centre (0.05, 0.05), radius 4.05 — AD24 omits the zero integer keys and stores frac-only) | fill/area colour, is_not_accessible=false, display flags |
| Rectangle | plain rects; ✅ transparent (`SHAPESTYLE`), ✅ GraphicallyLocked (`LOCKFLAGS`); ✅ `_Frac` coords incl. negatives (`FRACSHAPES`: (-5.45, -2.45)–(5.55, 2.55)) | line_style; 🚫 Disabled/Dimmed (authored but not persisted by AD24) |
| RoundRect | plain rounded rects | line_style, display flags; 🚫 transparent (authored but not persisted on a library round-rect) |
| Ellipse | plain ellipses; ✅ transparent (batch 3) | display flags |
| Polyline | plain polylines | line_style, start/end shapes, transparent, display flags |
| Polygon | plain polygons; ✅ transparent (`SHAPESTYLE` triangle) | is_not_accessible=false, display flags (line_style: N/A — `ISch_Polygon` has no LineStyle in AD24) |
| Pie | ✅ authored (`PIESYM`: 30–210°, radius 5 units, yellow fill, exact-asserted) | transparent, display flags, `_Frac` coords |
| Image | ✅ authored (`IMAGESYM`: bounding box, `logo.bmp`, KeepAspect, non-embedded); ✅ embedded image bytes in the `/Storage` stream (`EMBIMGSYM`, exact-asserted against the committed `embed.bmp`) | show_border non-default, display flags |
| Bezier | ✅ authored (`BEZIERSYM`, four control points exact-asserted) | non-default colour/width, display flags |
| Label | plain labels; ✅ justification variants + rotation (`JUSTIFY`) | mirror, display flags |
| Parameter | Value etc. | justification, orientation, is_mirrored, autoposition, is_configurable, is_rule/is_system, area colour |

### Cross-cutting (both formats)

- **Universal display/lock flags** — `GraphicallyLocked` is golden-covered on Rectangle
  (`LOCKFLAGS`); `Disabled`/`Dimmed` are 🚫 documented AD24 negatives (not persisted on
  library shapes); `OwnerPartDisplayMode` at a non-default value remains self-round-trip only.
- **`unique_id`** — present in fixtures, so identity read is covered; but per-primitive GUID
  streams for populated cases are thin.
- **Fractional coordinates** — the Pin `_Frac` path is golden-covered via the `PinFrac` aux
  stream (`FRACPINS`); the text-record `*_Frac` key path on graphic shapes is now ✅
  golden-covered too (`FRACSHAPES`, batch 4a). The golden exposed a real convention:
  AD24 stores negative off-grid coordinates as **truncation toward zero with a SIGNED
  `_Frac`** (`Location.X=-5|Location.X_Frac=-45000` = −5.45) and **omits a zero integer
  key** when only the fraction is non-zero (`Location.X_Frac=5000` with no `Location.X`).
  The reader previously parsed `_Frac` as unsigned and silently truncated every negative
  off-grid coordinate; reader and writer now follow the signed toward-zero convention
  (see `docs/SCHLIB_FORMAT.md` § Fractional coordinates).

## Remaining enrichment backlog (batch 4+)

PcbLib: pad mask expansion; an embedded-STEP `ComponentBody`; a multi-layer spread
footprint (internal-plane / mechanical / drill / keepout layers). Pad thermal-relief /
power-plane is 🚫 blocked on the scripting side (native crash on a fresh library pad —
see the Pad row); a golden needs a different pad-init sequence. SchLib: non-default
`OwnerPartDisplayMode`. Each batch: extend the `.pas` → regenerate locally → commit
binaries → exact assertions.
