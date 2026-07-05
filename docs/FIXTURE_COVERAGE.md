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

Every "medium confidence / no golden fixture, self-round-trip only" caveat in recent PRs
(PinFrac, PinSymbolLineWidth, inverted-rect text, barcode, …) traces to the **golden fixtures
being too plain**. The fix is to enrich them.

## How the fixtures are produced (fully automated)

- `scripts/Generate-Samples.ps1` — launches Altium headless (RunScript CLI), runs the
  DelphiScript, copies the authored libraries into `scripts/samples/`.
- `scripts/altium/generate/GenerateSamples.pas` — the **authoring logic** (editable here;
  DelphiScript). Header declares it *iterative by design*: generate → read back → add the next
  feature → regenerate, until coverage is complete.
- **The one on-site step:** running the wrapper needs Altium installed (AD24). Division of
  labour — extend `GenerateSamples.pas` + add read tests in-repo; the maintainer runs
  `.\scripts\Generate-Samples.ps1` on their Altium box and commits the regenerated binaries.

## Coverage map

Legend: ✅ authored + asserted · 🟡 authored, weak/no assertion · ❌ not exercised (self-round-trip only).

### PcbLib (`footprints.PcbLib`)

| Primitive | Exercised today | Not exercised (❌) |
|-----------|-----------------|--------------------|
| Pad | shape (round/rect/oct/rrect), TH holes (round/square/slot), local stack, rotation, negative/far coords | thermal-relief (relief_*), power-plane connect style, paste/solder-mask expansion, testpoint flags, slot geometry on SMD, drill tolerances, jumper id, locked/keepout flags |
| Via | simple TH, two pad/hole sizes | thermal-relief, power-plane, tenting flags, net index, paste-mask expansion, GUID |
| Track | silk box + copper track, two widths, two layers | locked/keepout flags, net index |
| Arc | full circle + quarter arc | fill/area colour, locked/keepout, net index |
| Region | copper box + mechanical box | KIND (cutout/…), net, name, arc-resolution/cavity/subpoly/union params |
| Fill | axis-aligned + 45°-rotated copper | locked/keepout, net index |
| Text | stroke text, Win-1252 chars, vertical (90°) | **mirror, bold, TrueType font_name, italic, justification, kind=TrueType/BarCode, stroke_font, inverted-rect block, barcode block, char_set, union_index, net/component index, flags** |
| ComponentBody | one extruded box (Mechanical) | cavity height, model 2D location/rotation, non-default colour/opacity, raw-outline precision |

### SchLib (`symbols.SchLib`)

| Primitive | Exercised today | Not exercised (❌) |
|-----------|-----------------|--------------------|
| Pin | electrical types (all 8), orientations (0/90/180/270), name/designator visibility, edge decorations, dual-part `owner_part_id` | **PinFrac (fractional coords), PinSymbolLineWidth, owner_part_display_mode, swap_id_group, part_and_sequence, default_value, graphically_locked** |
| Line | plain segments | line_style (dashed/dotted), is_not_accessible=false, display flags |
| Arc | plain arcs | fill/area colour, is_not_accessible=false, _FRAC coords, display flags |
| Rectangle | plain rects | line_style, transparent, display flags |
| RoundRect | plain rounded rects | line_style, transparent, display flags |
| Ellipse | plain ellipses | transparent, display flags |
| Polyline | plain polylines | line_style, start/end shapes, transparent, display flags |
| Polygon | plain polygons | **line_style, transparent, is_not_accessible=false, display flags** |
| Label | plain labels | justification variants, mirror, rotation, display flags |
| Parameter | Value etc. | justification, orientation, is_mirrored, autoposition, is_configurable, is_rule/is_system, area colour |
| Bezier | *(not authored at all)* | entire primitive |

### Cross-cutting (both formats)

- **Universal display/lock flags** (`GraphicallyLocked` / `Disabled` / `Dimmed` /
  `OwnerPartDisplayMode`) — modelled on all 9 SchLib shapes, but the fixture authors none
  non-default, so their *read* path is self-round-trip only.
- **`unique_id`** — present in fixtures, so identity read is covered; but per-primitive GUID
  streams for populated cases are thin.
- **Fractional (`*_Frac`) coordinates** — the whole `_FRAC` read path is unexercised because
  every fixture primitive sits on the integer grid.

## Plan to close it

1. Extend `GenerateSamples.pas`: one component (or one small library) per feature area
   authoring the ❌ properties above — a mirrored/inverted/barcode text, an off-grid pin with
   a symbol line width, locked/dimmed shapes, non-default justifications, thermal-relief pads,
   filled arcs, cutout regions, a Bezier, etc.
2. Add read-assertion tests in `tests/samples_{schlib,pcblib}.rs`, each **guarded** to skip
   gracefully until the regenerated fixture carries the field (so CI stays green pre-regen).
3. Maintainer runs `.\scripts\Generate-Samples.ps1` → commits the enriched binaries.
4. The guarded tests activate → every field becomes verified against a real Altium file,
   retiring the self-round-trip caveats.
