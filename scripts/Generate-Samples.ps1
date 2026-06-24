<#
.SYNOPSIS
    On-site: author the golden .PcbLib / .SchLib sample libraries with a real Altium.

.DESCRIPTION
    Launches Altium with the GenerateSamples DelphiScript via the RunScript CLI, which
    authors reference libraries into the bridge directory; this wrapper then copies the
    saved libraries into scripts\samples\ for committing. These goldens are the ground
    truth for the Rust reader / round-trip tests.

    The RunScript launch + file-based bridge are adapted from coffeenmusic/altium-mcp
    (MIT) — https://github.com/coffeenmusic/altium-mcp

    On-site only: needs Altium Designer installed (developed against AD24). Never CI.

.PARAMETER AltiumExe
    Path to X2.EXE. Read from scripts\.env.local (ALTIUM_EXE) when omitted.

.PARAMETER TimeoutSeconds
    How long to wait for Altium to finish authoring (default 300).

.EXAMPLE
    .\Generate-Samples.ps1
#>
param(
    [string]$AltiumExe,
    [int]$TimeoutSeconds = 300
)

$ErrorActionPreference = 'Stop'

$BridgeDir    = 'C:\Users\Public\altium_designer_mcp\samples'
$ResponseFile = Join-Path $BridgeDir 'generate_response.json'
$ScriptDir    = Split-Path -Parent $MyInvocation.MyCommand.Path
$PrjScr       = Join-Path $ScriptDir 'altium\generate\GenerateSamples.PrjScr'
$SamplesDir   = Join-Path $ScriptDir 'samples'

if (-not (Test-Path $PrjScr)) { throw "Generate project not found: $PrjScr" }

# Resolve X2.EXE from .env.local at the repo root (no auto-discovery — multiple
# Altium versions may be installed).
. (Join-Path $ScriptDir 'Resolve-AltiumExe.ps1')
$AltiumExe = Resolve-AltiumExe -Override $AltiumExe -EnvFile (Join-Path (Split-Path -Parent $ScriptDir) '.env.local')
Write-Host "Altium : $AltiumExe"

# Fresh bridge dir; clear any stale response.
New-Item -ItemType Directory -Force -Path $BridgeDir | Out-Null
if (Test-Path $ResponseFile) { Remove-Item $ResponseFile -Force }
Write-Host "Authoring golden libraries..."

# Launch Altium with the generate script. The `^|` separator is what RunScript expects;
# routed through a .bat for reliable pipe/quote passing (matches coffeenmusic/altium-mcp).
$bat = Join-Path $env:TEMP 'altium_designer_mcp_generate_launch.bat'
"`"$AltiumExe`" -RScriptingSystem:RunScript(ProjectName=`"$PrjScr`"^|ProcName=`"GenerateSamples>Run`")" |
    Set-Content -Path $bat -Encoding ASCII
Start-Process -FilePath $bat -WindowStyle Hidden

# Poll for the response.
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
while (-not (Test-Path $ResponseFile) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 500 }
if (-not (Test-Path $ResponseFile)) {
    throw "Timed out after $TimeoutSeconds s waiting for Altium. If a modal dialog is open, dismiss it."
}

$result = Get-Content $ResponseFile -Raw | ConvertFrom-Json
if ($result.status -ne 'ok') {
    throw "Altium generate script error: $($result.detail)"
}
Write-Host "Altium : $($result.detail)" -ForegroundColor Green

# Copy the authored libraries into scripts\samples\ for committing.
New-Item -ItemType Directory -Force -Path $SamplesDir | Out-Null
$libs = Get-ChildItem -Path $BridgeDir -File | Where-Object { $_.Name -match '\.(PcbLib|SchLib)$' }
if (-not $libs) { throw "No libraries were produced in $BridgeDir." }
foreach ($lib in $libs) {
    Copy-Item $lib.FullName -Destination (Join-Path $SamplesDir $lib.Name) -Force
    Write-Host ("  -> samples\{0}" -f $lib.Name) -ForegroundColor Green
}
Write-Host "`nGolden libraries written to $SamplesDir." -ForegroundColor Green
