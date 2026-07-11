# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

Everything below ships in the first tagged release (the version heading and date are
stamped when the tag is cut). An MCP server that gives AI assistants file I/O and
primitive-placement tools for Altium Designer `.PcbLib` (footprint) and `.SchLib`
(symbol) libraries.

### Added

- **34 MCP tools** covering read/write, inspect/visualise (ASCII previews, style
  extraction), compare/diff, edit-in-place (component/pad/primitive updates, batch
  operations), component management (copy/rename/merge/reorder, cross-library),
  library operations (validate/repair, JSON/CSV export + import, `.LibPkg` project
  generation, embedded STEP extraction) and automatic timestamped backups with
  restore. See `docs/TOOLS.md` for the full generated reference.
- **PcbLib**: all eight footprint primitives (Pad, Via, Track, Arc, Region, Text,
  Fill, ComponentBody) modelled byte-identically to Altium's own output, including
  pad stacks and slot holes, thermal-relief/power-plane connection, solder/paste
  mask control, TrueType/barcode/inverted text, region kinds, embedded STEP models
  and 3D body handling.
- **SchLib**: every record type that occurs in a real symbol library — pins (with
  swap groups, symbol decorations and auxiliary streams), all graphic shapes
  (rectangles, rounded rectangles, lines, polylines, polygons, arcs, elliptical
  arcs, ellipses, pies, Béziers), images (including embedded image bytes in the
  `/Storage` stream), text frames, labels, text, parameters and footprint links —
  with fractional (off-grid) coordinate support and multi-part/display-mode symbols.
- **Safety**: path confinement to configured `allowed_paths`, path-sanitised error
  messages, automatic pre-mutation backups (5 retained), dry-run previews, token-
  bucket rate limiting on mutating tools and an optional append-only audit log.
- **Verification**: a strict independent Altium-readability oracle (pyaltiumlib) in
  CI, Altium-authored golden fixtures with exact assertions, byte-identity tests
  against captured Altium templates, and no-panic property tests over hostile input.
