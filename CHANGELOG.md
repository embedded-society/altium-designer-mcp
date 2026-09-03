# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added

- **A `Dockerfile`.** `docker build -t altium-designer-mcp .` produces the same
  `--locked` release build as the published binaries on a minimal Debian image,
  running unprivileged with the mounted `/libraries` folder as its whole
  allow-list — for a Linux box, a NAS or a CI job that generates libraries into
  a mounted folder. The builder's Rust tag is held to `rust-toolchain.toml` by
  the same guard as the workflows, and Dependabot tracks the base images.

### Fixed

- **Dependabot can evaluate the repository's Python dependency files again.**
  A placeholder `requirements.txt` in the Altium generator directory (whose
  scripts need only the standard library) read `TODO: fill out this`, which
  Dependabot parsed as an invalid requirement and gave up on the whole
  ecosystem. The file is removed; the readability oracle's pinned
  requirements are the only Python dependencies.

## [1.0.1] - 2026-09-02

### Added

- **Every tool carries MCP annotations.** `tools/list` now gives each tool a
  `title` and `annotations` (`readOnlyHint`, `destructiveHint`,
  `openWorldHint`), derived from the same mutating-tool classification the
  rate limiter and audit log use — so a client knows before calling which of
  the 34 tools change files. `docs/TOOLS.md` shows the same marker per tool.
- **A privacy policy, and an icon on the Claude Desktop extension.** README
  gains a Privacy Policy section (the server collects nothing and never
  touches the network); the extension manifest links to it and carries the
  Embedded Society crest as its icon — both prerequisites for directory
  listings.
- **Every published release is listed in the official MCP Registry.** A
  workflow runs when a release goes public (or by hand for a tag), stamps
  `server.json` with the version and the bundle's SHA-256 re-derived from the
  release asset, and publishes it as
  `io.github.embedded-society/altium-designer-mcp` over GitHub's OIDC token —
  the registry the MCP clients' server browsers read.

## [1.0.0] - 2026-09-02

The first stable release. `altium-designer-mcp` reads and writes Altium Designer
`.PcbLib` and `.SchLib` libraries the way Altium does: every record layout is
verified against libraries authored by Altium Designer 24 and opened there again,
and a library read and written back comes out byte-identical. It exposes 34 tools
over MCP, wires into every MCP client we know of, and installs into Claude Desktop
with one click. From here on the tool interface — argument names, accepted values
and result shapes — follows semantic versioning: a change that breaks a caller
means a new major version.

### Added

- **Claude Desktop one-click extension.** Every release now ships
  `altium-designer-mcp.mcpb` — and the identical bundle as
  `altium-designer-mcp.dxt`, the format's old name, so older Claude Desktop
  builds install it too (pattern from
  [coffeenmusic/altium-mcp](https://github.com/coffeenmusic/altium-mcp)).
  One bundle carries the binary for macOS, Windows and Linux; installing it
  asks for your library folders and passes them to the server as `--allow`
  grants, so no config file or JSON editing is needed. The bundle is packed
  and schema-validated by the official MCPB CLI in the release workflow,
  which also unpacks it again and speaks MCP to the bundled binary, and it
  is checksummed and provenance-attested like every other artefact.
- **`--allow <DIR>` grants on the command line.** Repeatable; adds to the
  config file's `allowed_paths`, and works with no config file at all — the
  other settings then take their defaults. A config file that is named but
  missing, unreadable or invalid still fails loudly.
- **Four more golden-fixture gaps closed with AD24-authored evidence.** The
  regenerated goldens now carry a pin in an alternate display mode
  (`DISPMODE` — the pin-record byte), a graphically locked pin (`LOCKFLAGS`
  — flag bit 0x40), a dashed rectangle (`SHAPESTYLE` — persists as
  `LineStyleExt` between `Corner.Y` and `LineWidth`, and the writer now
  places it there), and an extruded body whose outline is off-grid by raw
  internal units (`BODYPREC` — AD24 keeps the exact units, and so does the
  reader). The `TEXT_SPECIAL` inverted rectangle is now authored at an
  explicit 120×70 mil: AD computes the size lazily from the rendered extent,
  and a headless save caught it at 0. All validated from Altium's side via
  `Verify-Libraries.ps1 -Expect`.
- **The on-site Altium harness asserts what Altium resolves, not just that a
  file opens.** `AltiumVerify.pas` now reports per-component primitive counts
  (all 8 PcbLib primitive kinds, all 16 SchLib record kinds) through the
  bridge; `Verify-Libraries.ps1 -Expect <json>` holds a library to an
  expectations file (component names as a set, counts matched by name — Altium
  iterates in its own shortlex order); and `Verify-RoundTrip.ps1` additionally
  asserts every server-written footprint's pads and symbol's pin and rectangle
  from Altium's side. `scripts/samples/golden_expectations.json` pins the two
  goldens — validated against AD24, all 25 footprints and 88 symbols — and
  `tests/golden_expectations.rs` keeps it in step with the reader. Verified in
  passing: Altium's component iterator skips a hidden `Comment` parameter but
  yields hidden user parameters.

### Fixed

- **A `rounded_rectangle` in a `top_middle_bottom` stack's `per_layer_shapes`
  is refused, not degraded.** The rounding lives in per-layer corner-radius
  bytes that only a `full_stack` pad's block stores, so a TMB slot was
  written as plain round and read back changed without a word. Both the
  tool layer and the writer now refuse it, naming the pad, the entry and
  the rule.
- **A pad or via stack the record cannot store is refused, not quietly
  mended.** A `per_layer_shapes` entry that was not a shape became round; a
  corner radius outside 0-100 — the pad's own or a per-layer one — became
  none; a malformed `per_layer_sizes` / `per_layer_offsets` entry became a
  zero-size layer; an entry count that did not match the stack mode had the
  missing layers filled from the pad's main size and the extras ignored; and
  per-layer arrays on a `simple` pad, or `per_layer_diameters` on a `simple`
  via, were ignored altogether. Each is now refused, naming the pad or via,
  the array and the entry, and the rule: 3 entries `[top, mid, bottom]` for
  `top_middle_bottom`, 32 for `full_stack` (offsets and radii on a full stack
  only), 32 diameters for a stacked via. The via schema's claim that
  `top_middle_bottom` takes 3 diameters was wrong: Altium stores 32 for
  either stacked mode.
- **A value of the wrong JSON type is refused, not silently defaulted.**
  Every tool call is now type-checked against the tool's own schema, however
  deeply the value sits: `"filled": "true"` or `"width": "1.5"` used to read as
  absent and take the default, so a footprint came out with the wrong pad size
  and nobody was told. The error names the value by its path.
- **`read_pcblib` / `read_schlib` resolve `component_name` like every other
  tool, and say when it is not there.** The filter compared names
  case-sensitively — the one place that did — so `"lm358"` for `LM358`, or
  any misspelling, came back as an empty success with `returned_count: 0`
  and no word of why. Both now resolve the name regardless of case and
  answer with the spelling on file, and a name the library does not hold is
  an error naming the available components — the same words `get_component`
  and `update_component` use.
- **Every tool that looks up a component reports a miss the same way.**
  Seventeen lookups used six wordings — `Symbol 'X' not found in library`,
  `Footprint 'X' not found. Available: ["A", "B"]`, `Source component 'X'
  not found`, a five-name "include" hint — and most named nothing of what
  the library holds. All of them now say
  `Component 'X' not found in library. Available: A, B, C ... and N more`
  (`compare_components` names the file it searched, the cross-library
  tools the source library).
- **`update_component` is type-checked as thoroughly as `write_pcblib` /
  `write_schlib`, and every key the tools accept is in `tools/list`.** The
  tool's `footprint` and `symbol` were untyped objects, so the check above
  never reached inside them; and some forty keys the parsers accept were
  missing from the write schemas — the read-modify-write carriers (`guid`,
  `raw_params`, `primitive_order`, `header_params`, `extra_streams`,
  `raw_layer_id`, …), the barcode text fields, a via's diameter stack and
  bottom-face mask expansion, the universal display flags on beziers and
  elliptical arcs, the `vertices` and `hidden` aliases. One footprint schema
  and one symbol schema now serve both tools, a test pins them to the parsers'
  allow-lists key for key, and `pads` / `pins` are no longer claimed as
  required: neither tool ever required them, and a logo footprint or a power
  symbol legitimately has none.
- **A description the Altium 365 library importer would refuse is reported
  before the import fails.** That importer turns away a component whose
  description exceeds 256 characters, naming neither the library nor the
  component; Altium Designer itself opens and reads such a library whole. So
  a longer footprint, symbol or footprint-link description is written as
  asked, and every write, `validate_library` and the post-write validation of
  every mutating tool report a warning that names the component and says by
  how many characters to shorten it.
- **A whole float reaches the tool as the integer it is; a page is asked
  for with a usable `limit` and `offset`.** The argument check accepts
  `2.0` wherever an integer is expected, as JSON Schema requires, but every
  handler then read the field as an integer, got nothing, and took the
  default — `"limit": 2.0` returned everything, `"corner_radius_percent":
  25.0` wrote no radius. Whole floats are now rewritten to integers at
  dispatch, under every integer-typed field the schemas describe (a float
  beyond 2^53 is refused as no integer). And `limit: 0` (a page that never
  advances), a negative `limit` or `offset` (read as absent) are refused by
  name on `read_pcblib`, `read_schlib`, `list_components` and
  `list_step_models`.
- **An integer outside the range its field can hold is refused, not read as
  absent.** A negative `net_index`, a `font_id` of 0, a `line_width` of 300,
  an `owner_part_id` below -1: each was read by a handler expecting an
  unsigned byte or word, got nothing, and took the default. Every integer
  argument now states its range in the schema (`tools/list` and
  `docs/TOOLS.md` show it) and the dispatch check refuses a value outside
  it by path — `Argument 'symbols[0].labels[0].font_id' must be between 1
  and 255, got 0`. A test holds every integer field to stating its floor.
- **An empty `filepath` is refused as empty.** It was reported as "cannot
  create a file at the filesystem root" — the message for a path with no
  parent directory, which an empty path also happens to lack.
- **A GUID the writer cannot encode is refused, not silently dropped or
  replaced.** A primitive's or footprint's `guid` that was not 32 hex digits
  vanished from the file's identity stream without a word, and a pad's
  `identity_guid` / `identity_guid_b` was quietly swapped for a fresh random
  one. Each is now refused by record and key — `Pad '1' identity_guid
  'not-a-guid' is not a GUID ({XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX})` —
  while any spelling of the 32 hex digits is kept as given. A component body
  on a layer the model does not know is likewise refused instead of being
  placed on Top 3D Body, and a body that cannot be built is named by index
  like every other primitive.
- **A `|` in a text field is refused, not written to come back cut.** Altium's
  records are pipe-delimited with no escape, so a description, parameter,
  label, region name or any other record text containing `|` was written
  and then read back — by this reader and by Altium alike — cut at that
  point. Altium's own editors set the rule (the schematic editor stores a
  `|` as `¦`, U+00A6; the PCB editor writes it raw and reads it back cut),
  so both writers and every mutating tool refuse it by field — before any
  backup is made — naming the substitute for schematic text. Strings kept in binary fields — a pad
  designator, a PCB text's string, a pin's name and designator — may carry
  one.
- **A font name longer than a Windows face name is refused, not written
  cut short.** A text's `font_name` or `barcode_font_name` past 31 UTF-16
  units — all the record's field holds — was truncated on write without a
  word; it is now refused by field with its length.
- **A path that is neither a `.PcbLib` nor a `.SchLib` is refused in one
  voice.** Eighteen tools said it six ways (`Unknown file type`,
  `Unsupported file type: .csv`, `File has no extension`, …); all now say
  `Unsupported file type '.csv' for 'Parts.csv': expected .PcbLib or
  .SchLib`, and a path without an extension says so.
- **A solid symbol body no longer paints over the pin names inside it.**
  `write_schlib` recorded the order it happened to parse its input in — pins
  before rectangles — and replayed that as if it were an authoring order, so a
  `"filled": true` body went out after the pins and hid their names. Symbols
  authored from JSON now take the canonical write order, which leads with the
  body graphics. An explicit `primitive_order` (as `read_schlib` echoes for a
  read-modify-write) still overrides it, unchanged.
- **Saving a library is about 40× faster, opening one about 6×.** Both
  writers serialised straight into an unbuffered file, and a compound-file
  writer rewrites its sector and directory tables constantly — a disk round
  trip each time; the readers seeked through an unbuffered file the same way.
  A library is now built in memory and written once, and read into memory
  before it is parsed; the bytes are identical.
- **Reading and writing a library now scale linearly with its size.** The
  compound-file crate rebuilt the whole mini-stream chain on every access to
  a small stream and walked an unbalanced directory tree on every path
  lookup, so a library of `n` components cost a term in `n²`: 500 footprints
  opened in 135 ms where 50 took 4 ms. Both are fixed in a patched `cfb`
  the build pins until the fix is released upstream; 500 footprints now open
  in 20 ms, and the bytes written are identical.
- The performance tests assert what they can prove: that saving and opening
  scale linearly with library size (the accidental-quadratic guard, valid in
  any build), and absolute bounds only in an optimised build, which CI now
  runs. A wall-clock bound on a debug build measured the machine, not the
  code, and failed on a slow one.
- The old `docs/CLAUDE_CODE_GUIDE.md` and `docs/ANTIGRAVITY_GUIDE.md` addresses,
  still linked from search results and MCP directories, point at the merged
  `docs/CLIENT_SETUP.md` instead of a missing page.

## [0.2.0] - 2026-08-24

Everything in this release is the result of one campaign: hold the library
files we write to exactly what Altium itself writes, and refuse — rather than
quietly reinterpret — anything a caller gets wrong.

### Added

- **IEEE symbol support** (`RECORD=3`): the 35 schematic decorations Altium
  places from its IEEE toolbar, read, written and rendered.
- **`validate_library` checks 3D-model integrity**: a component body pointing at
  a model the library does not hold is an error, an embedded model no body
  references is a warning, in both formats.
- **A mutation-fidelity suite**: every mutating tool is held to leaving the
  components it did not touch byte-identical, and export→import and merge are
  held to reproducing the library byte-for-byte.
- **Corpus verification**: the round-trip suite can be pointed at a directory of
  real, Altium-authored libraries (`ALTIUM_CORPUS_DIR`) and holds every one of
  them byte-identical through a read-write cycle.
- **Layer names in any spelling**: every tool that takes a layer accepts Altium's
  own name (`Top Overlay`), the camel-case form (`TopOverlay`), any case, and
  either separator.
- **Documentation**: `docs/CLIENT_SETUP.md` (verified wiring for 17 MCP clients)
  and `docs/USAGE.md` (client-neutral workflows), a documentation index in the
  README, and link checking in CI.

### Changed

- **Breaking — a symbol's `text` array is now `ieee_symbols`.** What the format
  calls `RECORD=3` is an IEEE symbol, not a text annotation; free text on a
  symbol has always been the `label` record. Symbols that carried `text` should
  use `labels`.
- **Breaking — `read_pcblib` and `read_schlib` return the component's own JSON
  shape**, the same shape `write_*` accepts and `export_library` emits, with
  empty lists omitted. A read now replays through a write byte-for-byte.
- **Breaking — `export_library`'s CSV carries one count column per primitive
  kind**, replacing the previous partial column set.
- **An unrecognised value is refused, not defaulted.** Unknown tool arguments,
  unknown JSON keys, malformed records and unrecognised enum values (pad shape,
  hole shape, mask-expansion mode, stack mode, pin orientation and electrical
  type, text justification, region kind, layer names) are reported, naming the
  field and the accepted values. Previously a typo silently produced the default
  and the caller was told nothing.
- **Component names resolve the way the file does — regardless of case.** Two
  names differing only in case are one component to Altium and to the OLE
  directory, so every tool that creates one refuses the collision, naming the
  spelling on file; naming an existing component in another case never re-spells
  it.
- Tool descriptions and `docs/TOOLS.md` state what each tool reports and which
  fidelity-carrying fields it round-trips.

### Fixed

- **Round-trip fidelity.** Non-ASCII text grew a byte on every save (`WideStrings`
  are UTF-16 code units, not UTF-8 bytes); schematic records lost keys the reader
  had no field for; saves invented identities and reordered symbols, so two saves
  of one library differed. Every record is now replayed as read, and a save is
  deterministic.
- **Data that lived outside the component record was dropped**: embedded 3D
  models on merge and on export→import, external STEP references on write, and
  a footprint's models on a compact-mode read.
- **Reports that omitted primitive kinds.** `diff_libraries`, `compare_components`,
  `validate_library`, `extract_style`, `export_library`'s CSV, `list_components`
  and both ASCII renderers each covered only some kinds; all now walk the kind
  enums, so a kind cannot be missing from a report.
- **In-place edits overridden by sibling data**: a stacked pad's size or shape
  edit and a via's diameter edit did not reach the per-layer tables Altium
  actually draws from, and a primitive moved to another layer kept the byte or
  token naming the old one.
- **List edits that moved unrelated records**: deleting a parameter or a 3D body
  left the component's recorded primitive order one slot long, moving every
  later record.
- **Crashes and unsafe recovery**: a component name containing `:` panicked the
  process; a panicking tool now answers with an error instead of killing the
  server; `restore_backup` snapshots the current file and writes atomically.
- **Identity duplication**: a copied component carried its source's GUIDs and
  unique IDs.
- **Altium-specific storage details**: Mechanical 17–32 layers, region and body
  layer tokens, pad flag bits, the two forms of a non-embedded STEP reference,
  parameter key order, and non-ASCII pin names, all now stored the way Altium
  stores them.
- **Coordinates outside the safe range** are refused for every primitive kind
  before writing, rather than saturating silently in the file.

### Removed

- `docs/COVERAGE_AUDIT.md` and the two per-client setup guides, folded into the
  coverage map beside the samples and into `docs/CLIENT_SETUP.md`.

## [0.1.0] - 2026-08-18

An MCP server that gives AI assistants file I/O and primitive-placement tools
for Altium Designer `.PcbLib` (footprint) and `.SchLib` (symbol) libraries.

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
