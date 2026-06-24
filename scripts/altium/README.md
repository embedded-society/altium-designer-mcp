# Altium On-Site Automation

Tooling that drives a **real, locally-installed Altium Designer** to validate our writer's output
and (in future) to author authoritative reference libraries for reverse engineering.

> **⚠ Never run any of this in CI.** Everything here needs the Altium Designer GUI application and
> a valid licence, so it is an **on-site developer aid only**. CI continues to verify
> Altium-readability through the independent `pyaltiumlib` oracle in
> [`tests/integration/`](../../tests/integration/). Do not add these scripts to any GitHub Actions
> workflow.
>
> **Status:** `Verify-Libraries` is a first working version. The Altium scripting API is
> version-specific (developed against AD24 / `X2.EXE` 24.5.2.23), so expect a little on-site
> iteration — please report anything that needs adjusting.

## Why on-site

`pyaltiumlib` is an excellent *proxy* for "will Altium open this?", but only Altium Designer itself
is the final word. With Altium installed you can get a definitive answer locally, and (later)
capture first-party golden files that the format work is checked against.

## `Verify-Libraries.ps1`

Opens one or more generated libraries in real Altium and reports whether each loads.

```powershell
# From scripts/altium/ :
.\Verify-Libraries.ps1 -Files C:\tmp\Verify.PcbLib, C:\tmp\Verify.SchLib
```

What it does:

1. Auto-detects `X2.EXE` under `C:\Program Files\Altium\AD*\` (override with `-AltiumExe`).
2. Writes the paths to a bridge request file (`C:\Users\Public\altium_designer_mcp\verify_request.txt`).
3. Launches Altium via `X2.EXE -RScriptingSystem:RunScript(...)`, which runs
   [`verify/AltiumVerify.pas`](verify/AltiumVerify.pas).
4. That DelphiScript opens each library, records the result, and writes
   `verify_response.json`.
5. The wrapper polls for the response and prints `PASS` / `FAIL` per file.

**Caveat:** a genuinely corrupt library can make Altium raise a *modal* "catastrophic failure"
dialog that blocks the script. If the wrapper times out, that itself indicates the file did not
open — dismiss the dialog in Altium and inspect the file.

## Prerequisites

- Windows with **Altium Designer** installed and licensed (the app must be able to start and open
  documents).
- A library to test — generate one with the built MCP server, or point it at any `.PcbLib` /
  `.SchLib`.

## How it works (the bridge)

Altium exposes a DelphiScript engine launchable from the command line. A PowerShell wrapper writes
a request file, launches the script, and polls for a response file the script writes back — a
simple, robust file-based bridge. The launch invocation and this request/response pattern are
adapted from **[coffeenmusic/altium-mcp](https://github.com/coffeenmusic/altium-mcp)** (MIT), which
drives the live Altium application; our use is the inverse — verifying files we generate *offline*.

## Planned

- `New-GoldenLibraries.ps1` — drive Altium to *author* reference libraries (place a known set of
  primitives, save) so we have first-party golden files to byte-diff the writer against, and to
  **settle the reverse-engineering gaps that only real Altium can confirm**.
