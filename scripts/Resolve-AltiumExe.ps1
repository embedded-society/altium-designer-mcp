<#
.SYNOPSIS
    Resolve Altium's X2.EXE path from .env.local — shared by the on-site launchers.

.DESCRIPTION
    People may have several Altium versions installed, so we deliberately do NOT
    auto-discover (the wrong one could be picked). The path is read from the
    repo-root .env.local (gitignored, per-machine): copy .env.local.example to
    .env.local and set ALTIUM_EXE. An explicit -Override always wins.

    Dot-source this file, then call Resolve-AltiumExe.
#>
function Resolve-AltiumExe {
    param(
        [string]$Override,
        [Parameter(Mandatory = $true)][string]$EnvFile
    )

    if ($Override) {
        if (-not (Test-Path $Override)) { throw "Altium exe not found: $Override" }
        return (Resolve-Path $Override).Path
    }

    if (-not (Test-Path $EnvFile)) {
        throw "Config not found: $EnvFile`n" +
              "Copy .env.local.example to .env.local (repo root) and set ALTIUM_EXE=<path to X2.EXE>."
    }

    $line = Get-Content $EnvFile |
        Where-Object { $_ -match '^\s*ALTIUM_EXE\s*=' } |
        Select-Object -First 1
    if (-not $line) {
        throw "ALTIUM_EXE not set in $EnvFile (e.g. ALTIUM_EXE=C:\Program Files\Altium\AD24\X2.EXE)."
    }

    $exe = ($line -replace '^\s*ALTIUM_EXE\s*=', '').Trim().Trim('"')
    if (-not (Test-Path $exe)) {
        throw "ALTIUM_EXE path does not exist: $exe (check the repo-root .env.local)."
    }
    return (Resolve-Path $exe).Path
}
