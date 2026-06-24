# Sample libraries

Altium-authored reference libraries — the ground truth for the reader and round-trip
tests. **Generated on-site, never hand-edited:** run `scripts\Generate-Samples.ps1`,
which drives a real Altium Designer (via `altium\generate\GenerateSamples.pas`) to
author the libraries, then moves them here to be committed.

Committed as binaries (like [AltiumSharp](https://github.com/issus/AltiumSharp)'s `TestData`)
so CI can read them without Altium. Regenerate and re-commit whenever the authoring script's coverage grows.

> Building these is iterative — generate, read back with the Rust tests, extend the
> primitive set, regenerate. This folder is empty until the first sample set lands.
