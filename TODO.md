# TODO — Working Notebook

Durable task list for the post-reverse-engineering fix campaign and the on-site Altium tooling.

> **Where the detail lives:** the full byte-level findings are preserved on the
> `docs/format-re-findings` branch (and at `C:\tmp\re-out`); cross-session resume context is in the
> `format-re-effort` memory. This file is the actionable to-do list, not the findings dump.

## Status snapshot

- Comprehensive byte-level RE complete: **48 PcbLib + 121 SchLib** verifier-confirmed discrepancies
  and **~50 live-Altium gaps**. Answer key = AltiumSharp
  (`git clone --depth 1 https://github.com/issus/AltiumSharp.git`).
- Generated `.PcbLib` + `.SchLib` now **verified to open *and* render in real Altium 24** via the
  on-site harness (PASS) — the ground truth, not just the proxy.

---

## A. Format fix ladder (remaining RE findings)

> Per-field detail is exhaustive in `{pcb,sch}_discrepancies.json` (on the `docs/format-re-findings`
> branch). Below is **every record/area with outstanding work** so nothing is forgotten; severities:
> 🔴 hard failure · 🟠 round-trip/data · ⚪ byte-cosmetic.

### A1. PcbLib primitives (code bugs + missing features)

- [ ] 🟠 **Pad** — remaining (mostly golden / on-site / fidelity-gated): `is_plated` @SR5+60
      read-back; identity GUIDs @+126/+142 read-back; **multi-entry** full-stack tail (count>1);
      oblong/oval SMD pads should route to the 651 size/shape block (golden shows 651, we emit
      empty). *(slot length @+263 / hole rotation @+267 — DONE.)*
- [ ] 🟠 **Text**: `isComment` @40 / `isDesignator` @41 flags (still dropped on read).
      *(mirror @35, TrueType font-name @46-109, bold @44, inverted-rect @110-133 — DONE.)*

### A2. PcbLib stream / container layer

- [ ] ⚪ **PrimitiveGuids** sub-storage omitted; missing root streams (FileVersionInfo /
      EmbeddedFonts / LayerKindMap) in the File-Structure tree.

  *(SectionKeys + the non-empty WideStrings fidelity were triaged to §B — they need a real
      golden `.PcbLib` to settle.)*

### A3. SchLib records (code bugs + missing features)

Outstanding SchLib field fidelity all needs an Altium-authored golden to settle — see §B.

### A4. Doc rewrites (high correctness value, doc-only)

- [ ] `docs/PCBLIB_FORMAT.md`: rewrite **Via** (single 321-byte block), **Text** (252-byte),
      **Region** (single block + `holeCount`), **Fill** tail, **Track/Arc** extended tails,
      **ComponentBody** (one block; drop the invented SNAPCOUNT / Block 1-2).
- [ ] `docs/SCHLIB_FORMAT.md`: complete every per-record field table (omit-when-default, DXP units,
      `*_Frac`, BGR colours); binary-pin `Description` + `FormalType` + `SwapIdGroup` /
      `PartAndSequence` / `DefaultValue`.

### A5. Byte-order cosmetics (lowest value — last)

- [ ] ⚪ Canonical field order (`IsNotAccesible` first, etc.) + `F3` angle formatting (`360.000`) +
      omit zero-valued vertices, golden-verified. Reader is order-independent → pure byte-identity.

---

## B. Live-Altium gaps (~50) — now UNBLOCKED by the on-site harness

- [ ] Use the committed `scripts/samples` goldens (`footprints.PcbLib` + `symbols.SchLib`, authored
      on-site by `GenerateSamples.pas`) to settle the ~50 gaps the RE could not confirm: pad
      cache/marker bytes, implementation datafile index base (0 vs 1), `OwnerIndex` on
      RECORD=45/46/48, pin `FormalType` / SwapId aux-stream layouts, etc. (see
      `{pcb,sch}_live_altium_gaps.json`).
- [ ] 🔴 **SectionKeys** (long component names): format known (root stream, full-name→storage-key),
      but the faithful fix is a coupled `Library/Data` full-name flip + reader-reorder lockstep with
      an undecided `~NNN` collision rule — confirm the need against a real long-name `.PcbLib` first.
- [ ] **WideStrings Tier 2**: text SubRecord-1 offset-115 `wideStringIndex`, the dot/empty-filtered
      index base vs Altium's unfiltered, UTF-16-vs-Win1252 encoding, and the root-vs-per-component
      reader path — needs a real multi-text footprint as the oracle.
- [ ] **SchLib `IsNotAccesible` on Ellipse / Polyline**: these two emit no token today, so adding the
      field changes from-scratch bytes — confirm the default against a golden ellipse/polyline first.
- [ ] **SchLib Component (RECORD=1) header fields**: remaining is the component `UniqueID`
      (AltiumSharp carries it, but a fresh symbol would emit a brand-new random id — not
      byte-identical) and the dropped header fields (`DesignItemId` / `ComponentKind` / `LibraryPath`
      / `SheetPartFileName` / `IndexInSheet` / `OwnerPartId`) — need golden fixtures.
- [ ] **SchLib Designator position**: emit `Location.X/Y` instead of the hardcoded `Y=-6` / omitted
      `X`; the from-scratch default magnitude (Altium ~`X=-5, Y=5`) is unverified without a golden.
- [ ] **SchLib `IndexInSheet`**: AltiumSharp does NOT default it to `-1` per shape — the correct
      per-record value/emission needs a golden to confirm (the spec's `-1` design was wrong).
- [ ] **SchLib Implementation structural**: `MapDefiner` (RECORD=47) pin→pad map, the 46/48 payload
      bodies + cross-record `OwnerIndex` chain, honouring `IsCurrent` on write, `DataFileCount` > 1.
- [ ] **PcbLib ComponentBody remaining**: `IDENTIFIER` (comma-separated codepoint list — would
      misrender as a plain string), `MODEL.MODELTYPE`, `MODEL.SNAP*`, `TEXTURE`, and the non-default
      unit-formatted values (`ARCRESOLUTION`/`CAVITYHEIGHT`/`MODEL.2D.X/Y`) — need a golden to
      validate the formatting.
- [ ] Feed confirmed answers back into the fix ladder (especially the pad, A3).

## C. On-site Altium tooling

- [ ] *(Optional)* extend `Verify-Libraries.ps1` to assert primitive counts / specific properties,
      not just "opened".

## D. Release & distribution (no release exists yet)

- [ ] Cut the **first tagged release** so non-Rust users get a prebuilt binary (flagged in #68).
- [ ] Consider a `.dxt` Claude Desktop extension for one-click install (pattern from
      coffeenmusic/altium-mcp).

## E. Issues

- [ ] **#68** — core fixes shipped and generated libraries now verified to open in real Altium 24.
      Close once the reporter confirms (or declare verified on the strength of the on-site PASS).
- [ ] **#113** — via block (#114), round-trip fidelity, and extruded ComponentBody (#133) largely
      cover this; review the remaining items and close.

## F. Docs / AI workflow

- [ ] Enrich `docs/AI_WORKFLOW.md` with symbol pin-placement guidance (idea from coffeenmusic's
      `symbol_placement_rules.txt`).

---

## G. Feature + fixture completeness audit (2026-07-08)

**Goal:** *every* SchLib/PcbLib feature must be (1) implemented in the reader, (2) exercised by the
`scripts/samples` goldens, (3) asserted by a `tests/samples_*` test. Close **ALL** gaps, not just
high-value. Method: `cargo-llvm-cov` read-path coverage from the sample tests + primitive-set
cross-reference against AltiumSharp DTOs (`C:\tmp\AltiumSharp`, the answer key).

**Baseline read-path coverage from the sample tests** (`cargo llvm-cov test --test samples_schlib
--test samples_pcblib`): `schlib/reader.rs` 79.5% · `pcblib/reader/parsers.rs` 73.8% ·
`pcblib/read_io.rs` 64.3% · `pcblib/reader/mod.rs` 49.3% · `schlib/pin_aux.rs` 43.4% ·
`pcblib/reader/models.rs` **13.7%** · `pcblib/primitives/text.rs` **0%**. Every gap below is a
concrete slice of those uncovered lines.

### G1. SchLib — primitive set is COMPLETE ✓ (2026-07-11)

Every record type that occurs in a real `.SchLib` is read, written, tool-exposed and
golden-asserted (record IDs we handle: 1,2,3,4,5,6,7,8,9,10,11,12,13,14,28,30,34,41,44-48 +
the `/Storage` embedded-image bytes). Scope check settled by enumerating record IDs across
AltiumSharp's entire SchLib golden corpus — only `{1,4,6-14,28,34,41,44}` (+ binary pins)
ever occur:

- **OUT OF SCOPE (SchDoc-only, never occur in a symbol library — do NOT implement):**
  `ParameterSet` (43), `Note` (209), `CompileMask` (211), `Hyperlink` (226) — verified absent
  from every AltiumSharp SchLib golden and from `SchLibReader`'s expectations — plus Bus,
  BusEntry, Wire, Junction, NetLabel, NoErc, Port, PowerObject, SheetEntry, SheetSymbol,
  Template, Blanket, MapDefiner/List, Harness*, SignalHarness, Symbol(sheet).

### G2. PcbLib — primitive set is COMPLETE ✓ (gaps are field-level, tracked in §A1)

AltiumSharp `PcbLibReader` reads exactly: Arc, ComponentBody, Fill, Pad, Region, Text, Track, Via —
which is precisely our set. **No missing PcbLib primitive.** Remaining PcbLib work is field-level
(see §A1) plus the sample-coverage gaps in §G3.

### G3. Sample-coverage gaps (feature implemented, golden never exercises it → low read coverage)

Add each to `GenerateSamples.pas`, regenerate on-site, then assert exactly:

- [ ] **3D STEP model** (`pcblib/reader/models.rs` 13.7%): `BODY3D` only has a plain *extruded* body.
      Author a ComponentBody with an **embedded STEP model** (MODELID + model data stream) so the
      Model/STEP read path (parse_component_body model block, `read_io` model-stream) is covered.
- [ ] **Pad slot hole + tolerances** (`parsers.rs` ~1300/1370 uncovered): author a slot-hole pad
      (`AddThPad` already has `eSlotHole`? verify it sets slot length @263 / hole rotation @267) and a
      pad with drill tolerances.
- [ ] **Pad thermal-relief / power-plane / mask** (batch-4 target): needs the `GetState_Cache`→
      `SetState_Cache` pattern for mask expansion; corner-radius `CRPercentage[Layer]` crashes on a
      fresh Simple pad — find the correct pad-stack init first (documented in `GenerateSamples.pas`).
- [ ] **Via** net/power-plane/tenting + identity GUIDs — author a via carrying these.
- [ ] **PcbLib Text** inverted-rect + barcode (`primitives/text.rs` 0%, `parsers.rs` ~1080-1105):
      author an inverted (knockout) text and a barcode text.
- [ ] **Region** cutout kinds beyond board-cutout; named region.
- [ ] **Layer spread** (`reader/mod.rs` `layer_from_id` 379-484 mostly uncovered): the goldens place
      primitives on only a few layers, so most `layer_from_id` arms never run — author primitives
      across internal-plane / mechanical / drill / keepout layers (a single multi-layer footprint).
- [ ] **SchLib** `IsNotAccesible=false` on Ellipse/Polyline; pin swap_id / part_and_sequence /
      default_value at non-default values; multi-model Implementation (`DataFileCount>1`, `IsCurrent`).
- [ ] **PcbLib primitive flags** (locked / keepout / tenting) at non-default on each 2D primitive.

### G4. Execution plan

1. **Close the remaining §G1 gap** (Pie, Image incl. embedded `/Storage` bytes, and TextFrame are
   done): the in-scope verification of `ParameterSet`/`Hyperlink`/`Note`/`CompileMask`.
2. **Enrich the generator** (§G3) with the un-exercised features — one on-site regenerate per batch
   (kill `X2` between runs; a bad scripting name is a *compile* abort, a bad call can be a *runtime*
   ScriptingSystem.DLL crash — see the `altium-delphiscript-api-names` memory).
3. **Add exact `tests/samples_*` assertions** for every newly-exercised field; re-measure coverage
   until the reader modules approach 100%.
4. Fold in the §A1 field-level PcbLib gaps as their own format-EE PRs.
