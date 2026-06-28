# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added

- **Fractional (off-grid) SchLib coordinates.** All graphic primitives — lines,
  rectangles, rounded rectangles, arcs, ellipses, polylines, polygons, Béziers,
  labels, text, and parameters — now accept `f64` coordinates and round-trip
  through Altium's `<key>_Frac` companion fields. Integer-grid coordinates are
  unchanged on disk (no `_Frac` emitted). Pins remain integer-only. See
  [`docs/SCHLIB_FORMAT.md`](docs/SCHLIB_FORMAT.md#fractional-coordinates).

---

*Released versions will be documented here once the first official release is published.*
