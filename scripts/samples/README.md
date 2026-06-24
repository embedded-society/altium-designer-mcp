# Sample libraries

Altium-authored reference libraries — the ground truth for the reader and round-trip
tests. **Generated on-site, never hand-edited:** run `scripts\Generate-Samples.ps1`,
which drives a real Altium Designer (via `altium\generate\GenerateSamples.pas`) to
author the libraries, then moves them here to be committed.

Committed as binaries (like [AltiumSharp](https://github.com/issus/AltiumSharp)'s `TestData`)
so CI can read them without Altium. Regenerate and re-commit whenever the authoring script's coverage grows.

> Building these is iterative — generate, read back with the Rust tests, extend the
> primitive set, regenerate. Coverage grows component by component.

## Contents

Each component groups primitives that share one feature axis, so a failing read test
pinpoints the feature. Tests live in [`tests/samples_pcblib.rs`](../../tests/samples_pcblib.rs)
and [`tests/samples_schlib.rs`](../../tests/samples_schlib.rs).

| Library | Component | Exercises |
|---------|-----------|-----------|
| `pads.PcbLib` | `PAD_SHAPES` | Four SMD pads, one per pad shape: Round, Rectangle, Octagonal, RoundedRectangle |
| `pads.PcbLib` | `PAD_HOLES` | Three through-hole pads, one per hole shape: round, square, slot (square/slot exercise the 651-byte size/shape block) |
| `symbols.SchLib` | `PINS_ETYPE` | Eight pins, one per electrical type: input, bidirectional, output, open-collector, passive, hi-z, open-emitter, power |
