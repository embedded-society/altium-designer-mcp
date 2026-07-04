# Using altium-designer-mcp

Invariants for an agent driving this server. For the per-tool parameter schema
and a worked example of every tool, call `tools/list` (the schema travels with
the server) or read `docs/TOOLS.md`. This guide covers only the conventions that
the per-tool schema does not.

## Responsibility split

The **agent** supplies the intelligence — it computes footprint land patterns,
pin positions, and symbol geometry (from datasheets, IPC-7351, etc.). The
**tool** only validates and writes those primitives to the Altium binary
format. The server never invents geometry; pass it exact, finished numbers.

## Coordinate units (this trips up most callers)

- **`.PcbLib` (footprints):** millimetres. Pad/track/arc/region coordinates and
  sizes are all mm.
- **`.SchLib` (symbols):** schematic units where **10 units = 1 grid square**
  (≈ 100 mil). These are *not* millimetres. Graphic primitives accept fractional
  values; pin coordinates are integers.

## Pin geometry (the counter-intuitive one)

For a schematic pin, `(x, y)` is the **body-attach (inner) end** — the end that
touches the symbol body — **not** the connection tip. `orientation` is the
direction the pin **points outward**:

- `left`  → tip at `x - length` (pin sits on the body's left edge)
- `right` → tip at `x + length` (right edge)
- `up`    → tip at `y + length` (top edge)
- `down`  → tip at `y - length` (bottom edge)

So a left-side pin uses `"left"` with its `(x, y)` on the left edge of the body
rectangle; the tip extends further left. The `write_schlib` response echoes each
pin's computed `body_end`/`tip` so you can verify placement without opening
Altium.

## Overbar / active-low names

Pin, net, and parameter names are stored verbatim, so use Altium's overbar
convention: a backslash **after** each character to bar it. `R\E\S\E\T\` renders
as an overbarred RESET; `RW\` bars only the `W`. Only the barred characters take a
trailing `\`.

## Multi-part symbols

For a multi-unit component (dual/quad op-amp, gate array), set the symbol's
`part_count` and give each pin an `owner_part_id` = its 1-based part number. The
`DUALPART` sample shows pins split across two parts.

*Overbar + multi-part conventions per [coffeenmusic/altium-mcp](https://github.com/coffeenmusic/altium-mcp).*

## Filesystem sandbox

Only paths inside the configured `allowed_paths` (see `config.json`) can be read
or written. Requests outside the sandbox are rejected. Writes create a timestamped
`.bak` of any existing file first.

## Typical build flow

1. Gather the real pinout/dimensions (datasheet, land-pattern drawing).
2. Compute geometry (the agent's job).
3. `write_pcblib` for footprints, `write_schlib` for symbols.
4. Link each symbol to its footprint(s) via the symbol's `footprints` field
   (`name` + optional `library_path`).
5. `write_libpkg` to group the `.SchLib` + `.PcbLib` for compilation.

Compiling to an `.IntLib` is a one-click step inside Altium; this server only
produces the source documents.
