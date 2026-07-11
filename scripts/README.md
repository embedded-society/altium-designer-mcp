# Scripts

On-site developer tooling for the Altium binary formats. The **tooling** here (the PowerShell
launchers and DelphiScript) is for **manual, local use only** — it needs a real Altium and
**never runs in CI**. The committed [`samples/`](samples/) libraries it authors, however, *are*
read by the automated tests (`tests/samples_pcblib.rs`, `tests/samples_schlib.rs`) as golden
fixtures. (CI additionally verifies Altium-readability through the independent `pyaltiumlib`
oracle in [`tests/integration/`](../tests/integration/).)

| Path | What it is | Needs Altium? |
|------|------------|---------------|
| [`Verify-Libraries.ps1`](Verify-Libraries.ps1) | Launch Altium to confirm a `.PcbLib`/`.SchLib` opens cleanly | **Yes** |
| [`Generate-Samples.ps1`](Generate-Samples.ps1) | Launch Altium to author the sample libraries | **Yes** |
| [`Resolve-AltiumExe.ps1`](Resolve-AltiumExe.ps1) | Shared helper: read `ALTIUM_EXE` from the repo-root `.env.local` | — |
| [`altium/`](altium/) | The DelphiScript automation the launchers run | **Yes** |
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
| [`altium/verify/`](altium/verify/) | `AltiumVerify.pas` — opens each library and reports PASS/FAIL (run by `Verify-Libraries.ps1`) |
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

## References

Working on the DelphiScript automation in [`altium/`](altium/)? Altium's official scripting docs:

- [DelphiScript language guide](https://www.altium.com/documentation/altium-designer/scripting/delphiscript/support)
  — the language reference for the `.pas` scripts.
- [Scripting Examples Reference](https://www.altium.com/documentation/altium-designer/scripting/examples-reference)
  — worked examples (creating PCB/Schematic objects, saving documents, etc.).
- [Scripting API Objects](https://techdocs.altium.com/display/SCRT/Script+API+Objects)
  — the `IPCB_*` / `ISch_*` interface reference (note: last revised for an older AD version).
