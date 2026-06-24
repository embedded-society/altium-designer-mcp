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

- [ ] 🔴 **Pad** *(biggest — near last)*: real **596-byte** size/shape block (non-Simple pads are
      emitted under-length → **Altium rejects them**); octagonal id 3 ≠ Oval; `is_plated` @SR5+60;
      tri-state mask modes @+101/+102 (reuse `MaskExpansionMode`); slot length @+263 / hole rotation
      @+267; identity GUID @+126; middle/bottom sizes; full-stack tail (`636+count*15`); solder-mask
      template-default leak.
- [ ] 🟠 **Via**: identity GUIDs @259-274/@275-290; net/comp/power-plane/paste/drill-pair fields.
- [ ] 🟠 **Text**: `mirrored` @35 / `isComment` @40 / `isDesignator` @41; font-name fields
      @46-109/@161-224; InvertedRect template bytes @124-131. *(italic / baseFontType shipped #154;
      raw `fontId` @25 + justification @132 are custom-font / inverted-rect only — deferred.)*
- [ ] 🟠 **ComponentBody**: broad field coverage (colour/opacity/texture/2D-placement/identifier).

### A2. PcbLib stream / container layer

- [ ] ⚪ **PrimitiveGuids** sub-storage omitted; missing root streams (FileVersionInfo /
      EmbeddedFonts / LayerKindMap) in the File-Structure tree.

  *(SectionKeys + the non-empty WideStrings fidelity were triaged to §B — they need a real
  golden `.PcbLib` to settle. PRIMITIVEINDEX and the empty WideStrings form are shipped.)*

### A3. SchLib records (code bugs + missing features)

The without-Altium field fidelity is **shipped** (#126–#129, #147–#151: per-record fields,
elliptical-arc frac carry, PartCount floor, footprint `IsCurrent`, pin tail fields). Everything
still outstanding needs an Altium-authored golden to settle — relocated to §B.

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
- [ ] 🔴 **SectionKeys** (long component names): format known (root stream, full-name→storage-key),
      but the faithful fix is a coupled `Library/Data` full-name flip + reader-reorder lockstep with
      an undecided `~NNN` collision rule — confirm the need against a real long-name `.PcbLib` first.
- [ ] **WideStrings Tier 2**: text SubRecord-1 offset-115 `wideStringIndex`, the dot/empty-filtered
      index base vs Altium's unfiltered, UTF-16-vs-Win1252 encoding, and the root-vs-per-component
      reader path — needs a real multi-text footprint as the oracle.
- [ ] **SchLib `IsNotAccesible` on Ellipse / Polyline**: the other per-record fields shipped, but
      these two emit no token today, so adding the field changes from-scratch bytes — confirm the
      default against a golden ellipse/polyline first.
- [ ] **SchLib `*_FRAC` cross-cutting** (Location/Corner/X1..n on arc, ellipse, line, rectangle,
      roundrect, label, bezier, polyline): encoding known (`raw = DXP*100000 + FRAC`, signed), but
      every coord field is `i32` and can't hold sub-DXP precision — a faithful fix is a breaking
      `i32`→`f64` model/API change, and the per-record `_Frac` key spellings need a byte-compared
      golden. *(EllipticalArc radii already round-trip; the carry bug was fixed separately.)*
- [ ] **SchLib Component (RECORD=1) header fields**: the `PartCount` floor is fixed; remaining is
      the component `UniqueID` (AltiumSharp carries it, but a fresh symbol would emit a brand-new
      random id — not byte-identical) and the dropped header fields (`DesignItemId` / `ComponentKind`
      / `LibraryPath` / `SheetPartFileName` / `IndexInSheet` / `OwnerPartId`) — need golden fixtures.
- [ ] **SchLib Designator position**: emit `Location.X/Y` instead of the hardcoded `Y=-6` / omitted
      `X`; the from-scratch default magnitude (Altium ~`X=-5, Y=5`) is unverified without a golden.
- [ ] **SchLib `IndexInSheet`**: AltiumSharp does NOT default it to `-1` per shape — the correct
      per-record value/emission needs a golden to confirm (the spec's `-1` design was wrong).
- [ ] **SchLib Label/Text (RECORD=3/4)**: RECORD=3 is *Symbol* (glyph ref), not Text; `encode_text`
      writes RECORD=4, clashing with Label. The correct mapping is a byte-changing rework — validate
      against a golden.
- [ ] **SchLib Implementation structural**: `MapDefiner` (RECORD=47) pin→pad map, the 46/48 payload
      bodies + cross-record `OwnerIndex` chain, honouring `IsCurrent` on write, `DataFileCount` > 1.
      *(IsCurrent read-back shipped #150.)*
- [ ] **SchLib Pin aux streams**: `SymbolLineWidth` + `PinFrac` live in separate OLE streams, not the
      pin record — need golden byte offsets. *(FormalType / SwapId / DefaultValue tail shipped #151.)*
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
