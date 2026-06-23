# Format Reverse-Engineering Findings

Verified reverse-engineering of the PcbLib and SchLib binary formats, captured so the knowledge
is preserved in version control. **Working reference, not polished documentation** — the goal is
to fold the verified field layouts into [`PCBLIB_FORMAT.md`](../PCBLIB_FORMAT.md) /
[`SCHLIB_FORMAT.md`](../SCHLIB_FORMAT.md) and to drive code fixes, then retire this folder.

## How it was produced

A multi-agent reverse-engineering pass triangulated every record's byte layout across four
independent ground-truth sources and adversarially verified each discrepancy:

1. **AltiumSharp** (`issus/AltiumSharp`, the OriginalCircuit.Altium fork) — the de-facto answer
   key: DTOs + `Serialization/Binary/BinaryFormat{Reader,Writer}.cs`.
2. **Real golden bytes** — AltiumSharp's `TestData/Generated/Individual/{PCB,SCH}/*.{PcbLib,SchLib}`
   per-feature files, plus our own Altium-authored [`scripts/samples/`](../../scripts/samples/).
3. **Our Rust reader/writer**.
4. **Our existing format docs**.

Re-clone the answer key with: `git clone --depth 1 https://github.com/issus/AltiumSharp.git`

## Files

| File | Contents |
|------|----------|
| `PCBLIB_VERIFIED_SPEC.md` | Consolidated, verified field-by-field PcbLib spec (all records) |
| `SCHLIB_VERIFIED_SPEC.md` | Consolidated, verified field-by-field SchLib spec (all records) |
| `pcb_discrepancies.json` | 48 verifier-confirmed PcbLib discrepancies (our code/docs vs the answer key) |
| `sch_discrepancies.json` | 121 verifier-confirmed SchLib discrepancies |
| `pcb_live_altium_gaps.json` | PcbLib fields only a live Altium install can settle |
| `sch_live_altium_gaps.json` | SchLib fields only a live Altium install can settle |

Each discrepancy carries `record`, `field`, `severity`, `kind`
(`our-code-bug` / `our-doc-error` / `our-missing-feature`), `detail`, `evidence`, and a
`recommendation`.

## Key finding

Our writer is **byte-correct for the simple, from-scratch case it targets** (the readability
oracle stays 13/13). The bulk of the confirmed discrepancies are:

- **Functional** — e.g. the SchLib reader dropped shapes sitting on a zero coordinate, and fill
  (`IsSolid`) did not round-trip. (Fixed in #126.)
- **Byte-canonicalisation** — Altium omits zero/false keys (`AddNonZero`, booleans only when
  true), uses a specific field order, and formats angles as `F3` (`360.000`). Altium reads our
  output fine, but it is not byte-identical to Altium's own. (Aligns with the *byte-exact Altium
  output* design principle.)
- **Missing model fields** — several records need new fields (e.g. `IndexInSheet`, per-primitive
  identity GUIDs) to round-trip everything.
- **Live-Altium gaps** — a handful the RE itself could not settle (e.g. the implementation
  datafile index base); these need the on-site AD24 session, not a guess.

## Status

- **#126** — SchLib reader robustness (zero-coord defaulting) + `IsSolid` round-trip. Merged/​in
  review.
- Remaining batches: SchLib writer canonicalisation, PcbLib pad stack (596-byte block) and other
  primitive fixes, and the doc corrections — tracked for follow-up.
