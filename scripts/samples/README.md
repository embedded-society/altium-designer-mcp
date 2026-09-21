# Sample libraries

Altium-authored reference libraries — the ground truth for the reader and round-trip
tests. **Generated on-site, not hand-edited** (the exceptions are in `manual/` — see below): run `scripts\Generate-Samples.ps1`,
which drives a real Altium Designer (via `altium\generate\GenerateSamples.pas`) to
author the libraries, then moves them here to be committed.

Committed as binaries (like [AltiumSharp](https://github.com/issus/AltiumSharp)'s `TestData`)
so CI can read them without Altium. Regenerate and re-commit whenever the authoring script's coverage grows.

> Building these is iterative — generate, read back with the Rust tests, extend the
> primitive set, regenerate. Coverage grows component by component.

[`COVERAGE.md`](COVERAGE.md) is the map of what each fixture exercises, the enrichment
backlog, and the Altium behaviours verified not to persist.

[`golden_expectations.json`](golden_expectations.json) holds the component names and
per-component primitive counts this repo's reader finds in the two generated goldens, in
the shape `scripts\Verify-Libraries.ps1 -Expect` consumes — so an on-site Altium can
assert it resolves exactly the same, not merely that the files open. It is generated,
not hand-edited: `tests/golden_expectations.rs` fails when it drifts, and
`UPDATE_GOLDEN_EXPECTATIONS=1 cargo test --test golden_expectations` refreshes it after
a regeneration. Two Altium behaviours are baked in (evidence in
[`../README.md`](../README.md) § library iterators): the five damaged i18n names are
excused via `fixture_inconsistent` (Altium decodes those bytes differently by design),
and the parameter counts predict Altium's iterator, which skips a hidden `Comment`.

## NEVER open-and-save a committed golden in Altium

An AD load+save cycle silently damages fixtures (measured 2026-08-16 by resaving
`symbols.SchLib` through AD and diffing all 84 symbols with our reader):

- the five known-damaged i18n symbols (`_JV`, `_BN`, `_CR`, `_IU`, `_SB`) degrade
  *further* — AD's reader is the broken component, and each cycle compounds it;
- `ꆈꌠ_YI` (Yi) is a sixth, slower victim: intact in the committed golden, but one
  load+save turns its pin/label/parameter texts to mojibake;
- `PARAMS` loses a parameter value outright: `100nF` comes back as `*` — not an
  encoding issue, AD's own parameter handling.

Regeneration via `Generate-Samples.ps1` is fine (it authors from scratch); opening a
committed golden in the AD UI to "just fix one thing" and saving is not. Hand-fix work
happens in a fresh library committed under `manual/` instead.

## `manual/` — one-off fixtures, do NOT regenerate

`Generate-Samples.ps1` cannot produce everything. Some properties exist only in Altium's
UI and are not exposed on the scripting interfaces; others needed a question settled by a
small probe script of their own. Those fixtures live in `manual/`, each made once — in the
UI or by its probe — and committed as-is.

**`Generate-Samples.ps1` never touches this folder** — it only copies its own outputs over
the two top-level libraries. Equally, nothing regenerates these files: if one is deleted,
it has to be rebuilt by hand from the recipe below.

### `manual/i18n5.SchLib`

Five symbols, one per script whose *generated* fixture is internally inconsistent
(`FIXTURE_INCONSISTENT` in `tests/golden_fidelity.rs`): Javanese `ꦗꦮ_JV`, Bengali
`রোধক_BN`, Cherokee `ᏣᎳᎩ_CR`, Inuktitut `ᐃᓄᒃᑎᑐᑦ_IU` and beyond-BMP Han `𠮷野_SB`. Each
carries its word in the component name, the description suffix, one pin's name, a text
label and a `Value` parameter — the same shape as the generated i18n symbols.

Hand-authored in the AD24 UI (2026-08-16) because that is the only route that bypasses
AD's broken decode of these byte sequences (four scripted attempts failed differently —
see the `DOCUMENTED NEGATIVE` in `GenerateSamples.pas`). The file is also ground truth
for the **UI-authoring convention**: plain record keys are ANSI `?` husks, the real names
live in `%UTF8%` twins as raw UTF-8 bytes, the CFB storage names are real UTF-16
(surrogate pair included), and pin names travel only in `PinWideText`.

**To rebuild it:** File → New → Library → Schematic Library; for each of the five,
rename the component by pasting the name, paste the description, place one pin
(designator `1`) with the word pasted as its Name, place a text string with the word,
and add a parameter `Value` with the word. Save ONCE as `i18n5.SchLib` and never re-open
it in Altium (see the load+save warning above).

### `manual/identifier.PcbLib`

One footprint, `BODY_IDENT`, with two extruded 3D bodies authored in the AD24 UI
(2026-08-16). It settles three things no scripted fixture could:

- **`IDENTIFIER` encoding**: comma-separated decimal UTF-16 code units — `BodyA` is
  `66,111,100,121,65` and `µΩ电` is `181,937,30005` (a character beyond the BMP is its
  surrogate pair: `manual/wide.PcbLib`).
- **UI-authored extruded bodies carry a `MODEL.*` group** (stable `MODELID`, checksum, real
  `MODEL.2D.X/Y` placement, `MODEL.MODELTYPE=0` + `MODEL.EXTRUDED.MINZ/MAXZ`, and
  `TEXTURESIZEX/Y=0.0001mil`), unlike script-authored ones, which carry none.
- **Per-save ID stability**: the library was saved twice from one unchanged in-memory state,
  and the twin files differ only in `DATE`/`TIME`/viewport — `MODELID`, `MODEL.CHECKSUM`,
  `ITEMGUID`, `REVISIONGUID` are stable, which is what let `golden_fidelity` stop excusing
  them. (The twin also showed AD reorders bodies between saves — match bodies by identifier,
  never index.)

**To rebuild it:** new PCB Library; rename the component `BODY_IDENT`; place two extruded
3D bodies (Place → 3D Body, type Extruded, draw a rectangle each); set Identifier `BodyA`
with Overall Height 1mm on one and Identifier `µΩ电` with 0.5mm on the other; save ONCE.

### `manual/i18n4.PcbLib`

Four footprints named outside Windows-1252, authored in the AD24 UI on a Windows-1250
machine (2026-09-15), each with one pad and its name pasted as the description: Cherokee
`ᏣᎳᎩ_CR_0402` (outside every legacy code page), `ČĐŽ_SL_0402` (inside Windows-1250, outside
Windows-1252), the 36-unit `ᏣᎳᎩ_CR_LONG_NAME_ABCDEFGHIJKLMNOPQRS` and the 33-unit
`SURROGATE_AT_THE_CAP_012345678𠮷野`, whose surrogate pair straddles the 31-unit storage
cap. It is ground truth for the **PcbLib UI-authoring convention**, which is not the SchLib
one:

- `PATTERN`, `DESCRIPTION`, the `Data` stream's leading name block, `Library/Data` and
  `SectionKeys` hold the machine's ANSI code page (`C8 D0 8E` for `ČĐŽ`), with `?` for every
  UTF-16 unit the page cannot hold — a surrogate pair becomes `??`.
- The real text rides in `UNICODE__PATTERN` and `UNICODE__DESCRIPTION` as comma-separated
  decimal UTF-16 code units (`𠮷野` = `55362,57271,37326`), and `UNICODE=EXISTS` opens and
  closes the parameter block — only when the text leaves ASCII (the ASCII-named
  `identifier.PcbLib` carries neither). No `%UTF8%` twin exists anywhere in a PcbLib.
- A name within the 31-unit cap is stored under its real Unicode name (`ᏣᎳᎩ_CR_0402`); a
  longer one under its ANSI form cut at 31 (`???_CR_LONG_NAME_ABCDEFGHIJKLMN`,
  `SURROGATE_AT_THE_CAP_012345678?`), and `SectionKeys` maps the ANSI forms.

The `samples_manual_i18n4_*` tests in `tests/samples_pcblib.rs` pin every byte of that, and
`manual_pcblibs_survive_a_round_trip` in `tests/golden_fidelity.rs` proves the library — like
every hand-authored PcbLib here — comes back byte-identical from a rewrite.

**To rebuild it:** new PCB Library; for each of the four, double-click the footprint in the
PCB Library panel, paste the name into Name and into Description, then place one pad
(Tools → New Blank Footprint for the next); save ONCE as `i18n4.PcbLib` and never re-open
it in Altium.

### `manual/region_hole.PcbLib`

One footprint, `REGION_HOLE`: a copper region whose 200 mil square outline has an 80 mil
square hole. Scripted in AD24 (2026-09-21) by `scripts/altium/probe/RegionHoleProbe.pas`,
which builds the outline the proven way (`MainContour.Replicate` → `SetOutlineContour`) and
adds the hole through `GeometricPolygon.AddContourIsHole(Contour, True)` — the scripted route
to a region hole the generator had not found. `samples_manual_region_hole_reads_exactly`
pins the outline and the hole, and `manual_pcblibs_survive_a_round_trip` the byte-identical
rewrite.

**To rebuild it:** run the probe through Altium's `RunScript`, with the dialog watcher on the
probe's own response file; it saves `region_hole.PcbLib` under
`C:\Users\Public\altium_designer_mcp\probe\`:

```powershell
Remove-Item C:\Users\Public\altium_designer_mcp\probe\* -ErrorAction SilentlyContinue
& "$env:ALTIUM_EXE" -RScriptingSystem:RunScript(ProjectName="scripts\altium\probe\RegionHoleProbe.PrjScr"^|ProcName="RegionHoleProbe>Run")
scripts\Watch-AltiumDialog.ps1 -ResponseFile C:\Users\Public\altium_designer_mcp\probe\probe_response.json
```

### `manual/wide.PcbLib`

One footprint, `WIDE`: two stroke texts and two extruded bodies whose text and
identifier are `µΩ电` (beyond U+00FF) and `𠮷` (beyond the BMP). Scripted in AD24
(2026-09-21) by `scripts/altium/probe/WideProbe.pas`, which settles three scripting
questions:

- A string *literal* reaches Altium as its UTF-8 bytes widened through the ANSI page,
  but a *character* literal keeps its UTF-16 unit: `#937` is `Ω`, and `#55362#57271`
  is `𠮷`. The hexadecimal form `#$D842` is not parsed as a character at all.
- A body's `Identifier` property crashes the script engine even for ASCII;
  `SetState_Identifier` sets it.
- Altium writes the identifier `𠮷` as `55362,57271`, its UTF-16 code units, like
  every other Unicode field in the format.

`samples_manual_wide_text_and_identifiers_read_exactly` pins the texts and the
identifiers, and `manual_pcblibs_survive_a_round_trip` the byte-identical rewrite.

**To rebuild it:** as for `region_hole.PcbLib`, with `WideProbe.PrjScr` and
`WideProbe>Run`; it saves `wide.PcbLib`.

### `manual/thermal_relief.PcbLib`

One footprint with seven through-hole pads (60 mil round, 30 mil hole), made in the AD24 UI
(2026-09-21) to decode the per-pad polygon-connect override. Pad 4 leaves Pad Stack → All
Layers → **Thermal Relief** unticked; each other pad ticks it and changes one or two settings
in the "Edit Polygon Connect Style" dialog:

| Pad | Setting |
|-----|---------|
| 3 | Relief, air gap 9 mil, conductor width 11 mil (also a 15 mil manual paste expansion) |
| 4 | box unticked: no override |
| 5 | Relief, air gap 6 mil, conductor width 14 mil, 2 conductors, 45 Angle |
| 6 | Direct Connect |
| 7 | Relief, Auto conductors |
| 8 | No Connect |
| 9 | Relief, Auto conductors, Min Distance ticked at 20 mil |

`samples_manual_pad_polygon_connect` pins every override,
`samples_manual_pad_polygon_connect_edits` shows the writer adds and removes one as Altium
does, and `manual_pcblibs_survive_a_round_trip` pins the byte-identical rewrite.

**To rebuild it:** File → New → Library → PCB Library; place the seven pads; for each, in
the Properties panel under Pad Stack expand **All Layers**, tick **Thermal Relief** and click
its link to set the row above; save ONCE as `thermal_relief.PcbLib`.

### `manual/pipe.SchLib` and `manual/pipe.PcbLib`

A symbol, `PIPESYM`, and a footprint, `PIPEFP`, given a `|` through Altium's scripting API
in AD24 (2026-08-30): the symbol's description, a parameter's name and value, and a label,
and the footprint's description. They show what Altium does with the
record separator. The schematic editor stores every `|` as `¦` (U+00A6); the PCB editor
writes it raw and reads the description back cut at it (`A|B=C` comes back as `A`).
`manual_pipe_fixture_shows_altium_stores_a_pipe_as_a_broken_bar` and
`manual_pipe_fixture_shows_altium_cuts_pcb_text_at_the_pipe` pin both, and the writers
refuse a `|` on that evidence.

**To rebuild them:** the probe was a one-off and is not committed. In a script, set those
fields on a new symbol and footprint to text containing `|` (`A|B=C`, `Val|ue`, `1|2`,
`x|y`), and save each library once.

### `manual/footprint_link.SchLib`

One symbol, `Component_1`, with one pin and one footprint link `R0402`, added in the AD24 UI
(2026-09-21) through Properties → Footprint → Add with the PCB library left on **Any**. It
shows what that route writes: a `RECORD=45` link with the datafile group and `IsCurrent=T`,
**no** `IntegratedModel`/`DatabaseModel` flags, and the dialog's status line
`Footprint not found` as the `Description`. `samples_schlib_manual_footprint_link_from_the_ui`
pins it.

**To rebuild it:** File → New → Library → Schematic Library; place one pin; in the Properties
panel under Footprint click **Add**, name it `R0402`, leave the PCB library on **Any**, OK;
save ONCE as `footprint_link.SchLib`.

### `manual/parameters.SchLib`

One component, `PARAMPROPS`, carrying three `RECORD=41` parameters that between them cover
the parameter properties the generated golden cannot reach:

| Parameter | Carries | Why it is here |
|-----------|---------|----------------|
| `TestParam` = `123` | `Justification=7`, `NotAutoPosition=T` | the generated golden omits both, because Altium omits a property left at its default |
| `Rule` | `Text=UNIONINDEX=0¦RULEKIND=Width¦…`, `Description`, `IsHidden=T` | a PCB design-rule directive parameter — proves a rule is identified by `Name=Rule` plus that payload, **not** by an `IsRule` flag |
| `Comment` = `*` | the default set only | the control: it shows which keys Altium omits when nothing is changed |

**To rebuild it:**

1. **File → New → Library → Schematic Library**, save as `parameters.SchLib`.
2. Rename the component to `PARAMPROPS` and draw anything (a rectangle and one pin);
   the graphics are irrelevant.
3. Add a parameter `TestParam` = `123`, **visible**. In its Properties:
   - **untick Autoposition** — ticked is the default and Altium then writes nothing;
   - set **Justification** to top-centre (the up arrow), which stores `Justification=7`.
4. Add a second parameter via the parameter list's **Add → Rule**, choose a
   *Max-Min Width* rule, leave the widths at 10 mil, and click **OK** (not Cancel — a
   cancelled dialog writes nothing).
5. Save, and copy the file here.

**To check it before committing** — prints every key Altium actually wrote per parameter:

```powershell
python -c "import olefile,re,sys;f=olefile.OleFileIO(sys.argv[1]);d=b''.join(f.openstream('/'.join(e)).read() for e in f.listdir() if e[-1]=='Data');r=[x for x in re.split(rb'(?=\|RECORD=)',d) if b'RECORD=41' in x];[print('---',sorted(set(k.decode() for k in re.findall(rb'\|([A-Za-z0-9._%]+)=',x)))) for x in r]" scripts\samples\manual\parameters.SchLib
```

`NotAutoPosition` and `Justification` must both appear on `TestParam`, or step 3 did not
take.

## `section_keys/` — a stream from an Altium Designer 21 library

`AD21_PCB_Lib.SectionKeys.bin` is the root `SectionKeys` stream (1305 bytes, 17 entries) that Altium
Designer 21.0.8.223 wrote for a 402-footprint library, contributed as a hex dump by the reporter of
[issue #507](https://github.com/embedded-society/altium-designer-mcp/issues/507). It carries footprint
names only. `src/altium/mod.rs` reproduces it byte for byte from those names: every entry is a name of
31 or more characters, four of exactly 31 as identity pairs, and each storage name is the name with
`*` mapped to `_` and cut at 31 — Altium's rule, which the writer follows.

## Contents

Each component groups primitives that share one feature axis, so a failing read test
pinpoints the feature. Tests live in [`tests/samples_pcblib.rs`](../../tests/samples_pcblib.rs)
and [`tests/samples_schlib.rs`](../../tests/samples_schlib.rs).

| Library | Component | Exercises |
|---------|-----------|-----------|
| `footprints.PcbLib` | `PAD_SHAPES` | Four SMD pads, one per pad shape: Round, Rectangle, Octagonal, RoundedRectangle |
| `footprints.PcbLib` | `PAD_HOLES` | Three through-hole pads, one per hole shape: round, square, slot (square/slot exercise the 651-byte size/shape block) |
| `footprints.PcbLib` | `VIAS` | Two simple through-vias (Top to Bottom), different pad/hole sizes |
| `footprints.PcbLib` | `PAD_STACK` | A multi-layer through-hole pad stack (top/mid/bottom shapes and sizes differ) |
| `footprints.PcbLib` | `TRACKS` | Five tracks: a 4-segment silk box + a wider copper track |
| `footprints.PcbLib` | `ARCS` | A full circle and a quarter arc |
| `footprints.PcbLib` | `REGIONS` | A copper box and a mechanical box (filled regions) |
| `footprints.PcbLib` | `FILLS` | Two top-layer copper fills, one axis-aligned and one rotated 45 degrees |
| `footprints.PcbLib` | `BODY3D` | A simple extruded 3D component body (rectangular outline + height) |
| `footprints.PcbLib` | `TEXT_STROKE` | Stroke-font strings, including a 90° rotation |
| `footprints.PcbLib` | `TEXT_WIN1252` | Stroke text with non-ASCII Windows-1252 glyphs (micro sign, plus-minus) that round-trip to UTF-8 |
| `footprints.PcbLib` | `EDGE` | Boundary-case pads: a 45° rotated rectangle, plus negative and large coordinates |
| `symbols.SchLib` | `PINS_ETYPE` | Eight pins, one per electrical type: input, bidirectional, output, open-collector, passive, hi-z, open-emitter, power |
| `symbols.SchLib` | `PINS_ORIENT` | Four pins, one per orientation: right, up, left, down |
| `symbols.SchLib` | `PINS_VIS` | Pins covering show-name/show-designator combinations plus a hidden pin |
| `symbols.SchLib` | `PINS_DECOR` | A clock or dot on each of the four IEEE decoration slots (inner/outer edge, inside, outside) |
| `symbols.SchLib` | `LINES` | Horizontal, vertical and diagonal lines |
| `symbols.SchLib` | `ARCS` | A full circle and a quarter arc |
| `symbols.SchLib` | `LABELS` | Free-text labels with different justifications and a rotation |
| `symbols.SchLib` | `PARAMS` | A visible and a hidden component parameter |
| `symbols.SchLib` | `DUALPART` | A two-part symbol; pins split across part 1 and part 2 |
| `symbols.SchLib` | `RECTS` | A filled and an unfilled rectangle |
| `symbols.SchLib` | `ELLIPSES` | A circle and an ellipse |
| `symbols.SchLib` | `POLYLINES` | A three-point open polyline |
| `symbols.SchLib` | `ROUNDRECTS` | A filled rounded rectangle |
| `symbols.SchLib` | `POLYGONS` | Two filled four-vertex polygon boxes |
| `symbols.SchLib` | `EDGE` | Boundary-case pins: large and negative coordinates, and a 35-character pin name |
