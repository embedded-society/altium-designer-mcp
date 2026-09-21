# Scripts

On-site developer tooling for the Altium binary formats. The **tooling** here (the PowerShell
launchers and DelphiScript) is for **manual, local use only** — it needs a real Altium and
**never runs in CI**. The committed [`samples/`](samples/) libraries it authors, however, *are*
read by the automated tests (`tests/samples_pcblib.rs`, `tests/samples_schlib.rs`) as golden
fixtures. (CI additionally verifies Altium-readability through the independent `pyaltiumlib`
oracle in [`tests/integration/`](../tests/integration/).)

| Path | What it is | Needs Altium? |
|------|------------|---------------|
| [`Verify-Libraries.ps1`](Verify-Libraries.ps1) | Launch Altium to confirm a `.PcbLib`/`.SchLib` opens cleanly; `-Expect` asserts the components and per-component primitive counts Altium resolves | **Yes** |
| [`Generate-Samples.ps1`](Generate-Samples.ps1) | Launch Altium to author the sample libraries | **Yes** |
| [`Watch-AltiumDialog.ps1`](Watch-AltiumDialog.ps1) | Run alongside `Generate-Samples.ps1`: catches the modal dialog a compile error or crash opens headlessly, prints the offending identifier and kills Altium | **Yes** |
| [`Verify-RoundTrip.ps1`](Verify-RoundTrip.ps1) | Write libraries through the MCP server, then check Altium resolves every component name | **Yes** |
| [`Verify-MaskCacheState.ps1`](Verify-MaskCacheState.ps1) | Write pads carrying each mask-expansion cache state, then show what Altium makes of them | **Yes** |
| [`Resolve-AltiumExe.ps1`](Resolve-AltiumExe.ps1) | Shared helper: read `ALTIUM_EXE` from the repo-root `.env.local` | — |
| [`ConvertFrom-WireName.ps1`](ConvertFrom-WireName.ps1) | Shared helper: decode a component name from Altium's on-wire form | — |
| [`altium/`](altium/) | The DelphiScript automation the launchers run | **Yes** |
| [`altium/probe/`](altium/probe/) | One-off DelphiScript probes that settle a single scripting question and author the `samples/manual/` fixture proving it (`RegionHoleProbe.pas`: a region with a hole; `WideProbe.pas`: text and identifiers beyond U+00FF) | **Yes** |
| [`release/`](release/) | `review-draft.sh` and `publish-draft.sh`: the hard-checking draft review and the guarded publish of `docs/RELEASING.md` steps 8 and 9 (need `gh`, `bash`, `python`) | No |
| [`samples/`](samples/) | Altium-authored sample libraries (ground truth for the tests) | No |

## Configuration — `.env.local`

The launchers do **not** auto-discover Altium, because multiple versions may be installed and
the wrong one could be picked. Copy the repo-root [`.env.local.example`](../.env.local.example)
to `.env.local` (gitignored, per-machine) and set the path to your `X2.EXE`:

```ini
ALTIUM_EXE=C:\Program Files\Altium\AD24\X2.EXE
```

Or pass `-AltiumExe <path>` to either launcher to override.

## `altium/` — on-site Altium automation

DelphiScript that drives a **real, locally-installed Altium Designer** (developed against AD24)
through Altium's `RunScript` CLI. Because it needs the GUI application and a licence, it
**cannot run in CI**.

| Path | Role |
|------|------|
| [`altium/verify/`](altium/verify/) | `AltiumVerify.pas` — opens each library and reports PASS/FAIL plus the component names and per-component primitive counts Altium resolved (run by `Verify-Libraries.ps1`) |
| [`altium/verify/`](altium/verify/) | `AltiumMaskCache.pas` — reports every pad's mask-expansion cache state and re-saves the library (run by `Verify-MaskCacheState.ps1`) |
| [`altium/generate/`](altium/generate/) | `GenerateSamples.pas` — authors the sample libraries (run by `Generate-Samples.ps1`) |

The `RunScript` launch and the file-based request/response bridge are adapted from
[coffeenmusic/altium-mcp](https://github.com/coffeenmusic/altium-mcp) (MIT).

## `samples/` — sample libraries

Altium-authored reference libraries, generated on-site by `Generate-Samples.ps1` and committed
as binaries (like [AltiumSharp](https://github.com/issus/AltiumSharp)'s `TestData`) so CI can read them without Altium. They are the
ground truth the reader and round-trip tests validate against. See
[`samples/README.md`](samples/README.md).

> Building the sample set is **iterative**: generate → read back with the Rust tests → extend
> the authoring script's primitive coverage → regenerate. The committed set currently holds
> `footprints.PcbLib`, `symbols.SchLib`, and the `embed.bmp` image the symbols embed; it grows
> with each authoring-script extension.

## Caveats

Hard-won behaviour of Altium, DelphiScript and Windows PowerShell 5.1. Each cost real
debugging time here; several made a *correct* change look broken, which is the expensive
kind. Read this before writing or trusting a script in this folder.

### Altium recomputes a rule-driven mask expansion on load

The mask-expansion tri-state is `TCacheState = (eCacheInvalid, eCacheValid,
eCacheManual)`. Only `eCacheManual` survives a trip through Altium.
`Verify-MaskCacheState.ps1` hands Altium a library whose three pads differ only in that
state and shows what comes back:

| written | Altium reports after load | Altium re-saves |
|---------|---------------------------|-----------------|
| `none` (0) + 0.0 | valid=1, 40000 (4 mil) | `from_rule`, 4 mil |
| `from_rule` (1) + 0.0 | valid=1, 40000 (4 mil) | `from_rule`, 4 mil |
| `manual` (2) + 7 mil | valid=2, 70000 | `manual`, 7 mil |

The first two are indistinguishable afterwards: Altium discards whatever number a
rule-driven pad carries and computes the expansion from its own rule. A zero expansion
paired with `eCacheValid` therefore does **not** suppress the mask opening — worth knowing
before treating a wrong state here as a fabrication risk. It is a fidelity bug, not an
output one.

The corollary for fixtures: a re-saved library is not byte-comparable to the one handed in
even when nothing was edited, because opening it resolves caches.

### Altium's own RTTI answers enum questions without a run

Delphi compiles enumeration names into RTTI, so the shipped binaries carry the exact
identifiers and their ordinals. Scanning them resolves a naming or ordinal question in
seconds, with no Altium launch and no risk of a bad identifier aborting a script:

```python
b = open(r'C:\Program Files\Altium\AD24\System\Advpcb.dll', 'rb').read()
i = b.find(b'eCacheInvalid')
print(b[i-40:i+70])   # .TCacheState....eCacheInvalid.eCacheValid.eCacheManual.RT_PCB
```

That one lookup established `TCacheState = (eCacheInvalid, eCacheValid, eCacheManual)`
— ordinals 0/1/2 — which is the tri-state behind pad and via mask expansion. Names appear
consecutively in declaration order, so the position in the list *is* the stored byte.

`Advpcb.dll` (PCB engine) and `AdvSch.dll` (schematic engine) hold the editor enums;
`ScriptingSystem.dll` holds the scripting wrappers. Use this before guessing an identifier
or inferring a default from observed bytes alone.

### DelphiScript truncates any codepoint above 255

`Chr(N)` for `N > 255` wraps modulo 256, so `Chr(937)` (Greek `Ω`) yields byte 169 (`©`)
and `Chr(20013)` (CJK 中) yields 45 (`-`). Non-Windows-1252 text is therefore **not
authorable with `Chr`**.

A **literal** in the `.pas` does work — `GenerateSamples.pas` is UTF-8 and authors
`Comp.Name := 'Резистор'` correctly. Use a literal, never `Chr`, for non-Latin text.

### `try/except` does not catch an unknown identifier

DelphiScript resolves identifiers at **compile** time, so a single unknown name aborts the
whole script before any of it runs — wrapping the assignment in `try/except` protects
nothing. The failure surfaces as a modal `Undeclared identifier: X` dialog, and because the
script never runs, *every* footprint in that run is missing, not just the one with the bad
name.

Consequences worth planning around:

- Verify a property or enum name before using it. The verified-name list lives in the
  project memory notes; shipping AD24 scripts are the other reliable source.
- Some fields are not direct properties at all. Pad mask expansion goes through the cache
  record (`Cache := Pad.GetState_Cache; Cache.PasteMaskExpansionValid := eCacheManual; …;
  Pad.SetState_Cache := Cache;`) — there is no `eMaskExpansion_*` identifier.
- Add one new field family per run. A batch of guesses tells you only the *first* bad name.

### DelphiScript flattens non-ANSI strings when concatenating

Building a response by string concatenation turns any non-ANSI character into `?`. This
made the bridge destroy exactly what it was measuring: a correct library was reported as
`????????` and looked like a failure in the code under test.

`AltiumVerify.pas` therefore emits `\uXXXX` escapes from `Ord()` rather than embedding
characters directly. **Any new script that reports text through the bridge must do the
same**, or its results cannot be trusted for anything outside ASCII.

### Altium's PCB scripting API returns names in their on-wire form

For a footprint whose name is outside Windows-1252, `IPCB_LibComponent.Name` returns the
name's UTF-8 bytes carried one char per byte (`Резистор_0402` comes back as
`Đ ĐµĐ·Đ¸ŃŃ‚ĐľŃ€_0402`), not the true string. This is **not** a defect in the file being
read: asking Altium for the names in its own authored `samples/footprints.PcbLib` returns
the identical string.

So a name comparison must accept that form. Decoding it back requires the **system ANSI
code page** (`[System.Text.Encoding]::Default`), not 1252 — the widening happens through
whatever ANSI page the machine runs, which on a non-Western install is not 1252.

`ISch_Component.LibReference` does not share this; symbol names come back as the true
string.

### The library iterators impose Altium's own order and parameter set

Verified against the two goldens (AD24, 2026-09-01), which is why
`Verify-Libraries.ps1 -Expect` matches components by **name**, never by index:

- `IPCB_Library.LibraryIterator_Create` returns components in **shortlex order**
  (name length, then alphabetical), not file order; the SchLib iterator's order
  differs from file order too.
- A component's iterator (`GroupIterator_Create` with an object-set filter on
  PCB, `ISch_Component.SchIterator_Create` on Sch) counts primitives exactly as
  the file stores them — all 25 footprints' and 88 symbols' counts matched our
  reader — with one exception: a **hidden `Comment` parameter is not yielded**
  (the `PARAMS` golden carries 3 parameter records, Altium iterates 2), while a
  hidden *user* parameter is (the `JUSTIFY` golden's hidden `Tol` comes back),
  and so is a visible `Comment` (every i18n symbol's).

### Regenerating the samples changes values that tests assert

`Generate-Samples.ps1` re-authors both libraries from scratch, so Altium mints fresh
identifiers each run. After regenerating, expect to update:

- the **`EMBSTEP` model GUID** in `tests/samples_pcblib.rs` — a new GUID every time;
- **component counts and name lists**, when the authoring script gains a symbol or
  footprint.

`UniqueID` values in the SchLib round-trip expectations are deliberately **normalised** to
`<UID>` rather than asserted literally, precisely so a routine regeneration does not
require hand-copying random letters. Prefer that pattern for anything else Altium
regenerates.

### Windows PowerShell 5.1 traps

The scripts here target 5.1 (the shell Altium boxes have), which has three sharp edges:

- **A `.ps1` containing non-ASCII needs a UTF-8 BOM.** Without one, 5.1 reads the file as
  ANSI and mangles the script's own string literals, usually as a parse error far from the
  real cause.
- **Never `2>&1` a native executable.** Redirecting stderr wraps each line in an
  `ErrorRecord`, so `$ErrorActionPreference = 'Stop'` treats a clean `cargo build` (exit 0
  with warnings) as a failure.
- **`Set-Content -Encoding utf8` writes a BOM.** A BOM at the start of a JSON config makes
  the server's parser reject the file. Use
  `[System.IO.File]::WriteAllText($path, $text, (New-Object System.Text.UTF8Encoding $false))`.

### A modal dialog blocks the bridge

A genuinely corrupt library can make Altium raise a modal "catastrophic failure" dialog,
which the `try/except` in a script cannot catch. The wrapper then times out waiting for the
response file — which is itself the signal that the file did not open. Do not read a
timeout as a harness bug without checking the Altium window first.

## References

Working on the DelphiScript automation in [`altium/`](altium/)? Altium's official scripting docs:

- [DelphiScript language guide](https://www.altium.com/documentation/altium-designer/scripting/delphiscript/support)
  — the language reference for the `.pas` scripts.
- [Scripting Examples Reference](https://www.altium.com/documentation/altium-designer/scripting/examples-reference)
  — worked examples (creating PCB/Schematic objects, saving documents, etc.).
- [Scripting API Objects](https://techdocs.altium.com/display/SCRT/Script+API+Objects)
  — the `IPCB_*` / `ISch_*` interface reference (note: last revised for an older AD version).
