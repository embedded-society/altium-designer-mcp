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
| `pads.PcbLib` | `TRACKS` | Five tracks: a 4-segment silk box + a wider copper track |
| `pads.PcbLib` | `ARCS` | A full circle and a quarter arc |
| `pads.PcbLib` | `REGIONS` | A copper box and a mechanical box (filled regions) |
| `pads.PcbLib` | `TEXT_STROKE` | Stroke-font strings, including a 90° rotation |
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
