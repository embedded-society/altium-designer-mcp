# TODO — Working Notebook

Durable task list for the post-reverse-engineering fix campaign and the on-site Altium tooling.

> **Where the detail lives:** the full byte-level findings are preserved on the
> `docs/format-re-findings` branch (and at `C:\tmp\re-out`); cross-session resume context is in the
> `format-re-effort` memory. This file is the actionable to-do list, not the findings dump.

## Ground rules

- British English, 4-space indent, Conventional Commits, trailer
  `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- **NEVER** push to `main` — branch + PR. One PR per coherent batch; admin-merge own green PRs.
- **DO NOT modify** `CHANGELOG.md` or `CODE_OF_CONDUCT.md` (off limits).
- Every change must keep green: `cargo fmt --all --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, the `pyaltiumlib`
  readability oracle (**13/13**), and `markdownlint-cli2 "**/*.md"`.
- Verify each finding against the actual code before fixing (the adversarial pass already filtered
  false positives, but spot-check). **Do NOT guess the live-Altium gaps** — settle them on-site.

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

- [ ] 🔴 **Pad** *(biggest — near last)*: real **596-byte** size/shape block (non-Simple pads are
      emitted under-length → **Altium rejects them**); octagonal id 3 ≠ Oval; `is_plated` @SR5+60;
      tri-state mask modes @+101/+102; slot length @+263 / hole rotation @+267; identity GUID @+126;
      middle/bottom sizes; full-stack tail (`636+count*15`); solder-mask template-default leak.
- [ ] 🟠 **Via**: mask-mode tri-state enum [DONE #140 — shared `MaskExpansionMode`, reused by the pad];
      identity GUIDs @259-274/@275-290; `solder_mask_expansion_back` @242-245; net/comp/power-plane/
      paste/drill-pair fields.
- [ ] 🟠 **Text**: flag word [DONE #132], `strokeWidth` @36 [DONE #141]; `mirrored` @35 / `isComment`
      @40 / `isDesignator` @41; italic @45 / baseFontType @43; font-name fields @46-109/@161-224;
      InvertedRect template bytes @124-131. *(raw `fontId` @25 + justification @132 are custom-font /
      inverted-rect only — verified deferred in #141.)*
- [ ] 🟠 **Region**: param string (no leading pipe + 8 canonical keys) [DONE #139]; `hole_count` @14 +
      hole contours (multi-contour regions). *(Empty-block + V7_LAYER token [DONE #124/#132/#133].)*
- [ ] 🟠 **ComponentBody**: MODELTYPE/EXTRUDED/MODELSOURCE/V7_LAYER [DONE #133]; `MODEL.CHECKSUM`
      round-trip; broad field coverage (colour/opacity/texture/2D-placement/identifier).
- [ ] ⚪ **Fill**: `v7_layer_id` @42-45 [DONE #139]; `solder_mask_expansion` @37-40 +
      `keepout_restrictions` @46.

### A2. PcbLib stream / container layer

- [ ] 🔴 **SectionKeys** stream never read/written (needed for long component names).
- [ ] 🟠 **UniqueIDPrimitiveInformation** `PRIMITIVEINDEX` 0-based vs Altium **1-based**.
- [ ] 🟠 **WideStrings** empty-encoding (blockLen=2 spurious leading pipe) + non-empty trailing-pipe
      / `.`-prefixed desync handling.
- [ ] ⚪ **PrimitiveGuids** sub-storage omitted; missing root streams (FileVersionInfo /
      EmbeddedFonts / LayerKindMap) in the File-Structure tree.

### A3. SchLib records (code bugs + missing features)

> Reader early-returns, `IsSolid` round-trip, canonical booleans, colour read+omit, arc
> `IsNotAccesible`, Parameter `UniqueID`, text booleans = **[DONE #126-129]**. Remaining:

- [ ] 🟠 **Designator**: model `Location.X/Y` (Altium `X=-5, Y=5`; we hardcode `Y=-6`, omit `X`).
- [ ] 🟠 **`IndexInSheet`** from a stored model field (default `-1`), not the write counter —
      cross-cutting across most records.
- [ ] 🟠 **Fractional coordinates** (`*_FRAC`, sub-DXP precision) — cross-cutting across arc,
      ellipse, ellipticalarc, label, line, rectangle, roundrect, bezier.
- [ ] 🟠 **Per-record missing model fields**: `IsNotAccesible` (line/ellipse/bezier/polyline/
      ellipticalarc), `LineStyle`/`LineStyleExt` (line/polyline/rectangle/roundrect), `Transparent`,
      `AreaColor` (arc/ellipticalarc).
- [ ] 🟠 **Label / Text**: RECORD=3 is *Symbol*, not Text — `encode_text` writes RECORD=4 (clashing
      with Label); map RECORD=3/4 correctly. (Colour/Orientation/Justification omit + IsHidden/
      IsMirrored placement.)
- [ ] 🟠 **Implementation**: `MapDefiner` (RECORD=47) modelling; `DataFileCount` derive/loop;
      `IsCurrent` only-when-true. *(1-based index + OwnerIndex → live-Altium gaps, section B.)*
- [ ] 🟠 **Component (RECORD=1)**: emit/read `UniqueID`; `PartCount.max(1)` flooring corrupts
      round-trip; read more header fields.
- [ ] 🟠 **Pin**: `FormalType` model field; `DefaultValue` tail string; `SwapId` / `PinFrac` /
      `SymbolLineWidth`.

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

- [ ] Build `scripts/altium/New-GoldenLibraries.ps1` (+ DelphiScript) that drives Altium to
      **author** reference libraries (place a known primitive set, then save).
- [ ] Use those golden files to settle the ~50 gaps the RE could not confirm: pad cache/marker
      bytes, implementation datafile index base (0 vs 1), `OwnerIndex` on RECORD=45/46/48, pin
      `FormalType` / SwapId aux-stream layouts, etc. (see `{pcb,sch}_live_altium_gaps.json`).
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

## Recently shipped (this campaign — for reference, not to redo)

- **#126–#129** SchLib: stop dropping shapes on zero coords, `IsSolid` round-trip, canonical
  booleans + `unique_id`/`is_not_accessible` fields, correct colour reads + omit-when-zero.
- **#130–#131** doc corrections: PcbLib structural (FileHeader 53 B, common-header fields, PcbFlags
  wire bits) + SchLib semantics (IsSolid, end-marker, implementation record labels).
- **#132** PcbLib: read the text flag word + emit regions as a single block.
- **#133** *(contributor @ande2407)* generic extruded ComponentBody + `component_bodies` MCP input.
- **#135–#136** `scripts/altium/` on-site verify harness + README "Prior Art & Acknowledgements";
  harness fixes (`REPLACEALL`, BOM, wrapper error-detection, leave-open).
- **#137** TODO.md rewrite after the comprehensive RE.
- **#139** PcbLib: fill `v7_layer_id` + canonical region parameter string (byte-fidelity).
- **#140** PcbLib: via solder-mask expansion as tri-state `MaskExpansionMode` (fixes wrong default).
- **#141** *(open)* PcbLib: text `stroke_width` @36 round-trip.
