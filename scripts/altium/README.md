# Altium On-Site Automation

Tooling that drives a **real, locally-installed Altium Designer** to validate our writer's output
and to author authoritative reference libraries for reverse engineering.

> **Status: planned, not yet implemented.** This folder is a placeholder describing the intended
> tooling — the scripts named below do not exist yet.
>
> **⚠ Never run any of this in CI.** Everything here needs the Altium Designer GUI application and
> a valid licence, so it is an **on-site developer aid only**. CI continues to verify
> Altium-readability through the independent `pyaltiumlib` oracle in
> [`tests/integration/`](../../tests/integration/). Do not add these scripts to any GitHub Actions
> workflow.

## Why on-site

`pyaltiumlib` is an excellent *proxy* for "will Altium open this?", but only Altium Designer
itself is the final word. A developer who has Altium installed can use the tooling here to get a
definitive answer locally, and to capture first-party golden files that the format work can be
checked against.

## Prerequisites

- Windows with **Altium Designer** installed (developed against AD24, `X2.EXE` 24.5.2.23).
- A valid Altium licence — the application must be able to start and open documents.
- For the verification flow, a built MCP server binary (`cargo build --release`).

## Planned tooling

| Script | Purpose |
|--------|---------|
| `Verify-Libraries.ps1` (+ `delphiscript/VerifyLibraries.pas`) | Generate a library with the built MCP server, open it in real Altium, and report whether Altium loads it cleanly (with component and primitive counts). The authoritative answer to "does our output open in Altium?" |
| `New-GoldenLibraries.ps1` (+ `delphiscript/GenerateGolden.pas`) | Drive Altium to *author* reference libraries (place a known set of primitives, then save) so we have first-party golden files to byte-diff the writer against. |

## How it will work

Altium exposes a DelphiScript engine that can be launched from the command line
(`X2.EXE -RScriptingSystem:RunScript(...)`). A script opens or builds a library, writes a result
file, and exits; a PowerShell wrapper launches Altium, waits for the result file, and reports
pass or fail.

Because the scripting API is version-specific and the application is a heavy GUI, these scripts
are expected to need a few rounds of on-site iteration to get right.
