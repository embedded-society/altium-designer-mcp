<#
.SYNOPSIS
    On-site: author the .PcbLib / .SchLib sample libraries with a real Altium.

.DESCRIPTION
    Launches Altium with the GenerateSamples DelphiScript via the RunScript CLI, which
    authors reference libraries into the bridge directory; this wrapper then copies the
    saved libraries into scripts\samples\ for committing. These samples are the ground
    truth for the Rust reader / round-trip tests.

    The RunScript launch + file-based bridge are adapted from coffeenmusic/altium-mcp
    (MIT) — https://github.com/coffeenmusic/altium-mcp

    On-site only: needs Altium Designer installed (developed against AD24). Never CI.

.PARAMETER AltiumExe
    Path to X2.EXE. Read from scripts\.env.local (ALTIUM_EXE) when omitted.

.PARAMETER TimeoutSeconds
    How long to wait for Altium to finish authoring (default 300).

.PARAMETER KeepAltiumOpen
    By default Altium is closed once authoring completes; pass this to leave it running.

.EXAMPLE
    .\Generate-Samples.ps1
#>
param(
    [string]$AltiumExe,
    [int]$TimeoutSeconds = 300,
    [switch]$KeepAltiumOpen
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
# Clear stale libraries first: Windows is case-insensitive, so saving 'symbols.SchLib'
# over an existing 'SYMBOLS.SchLib' keeps the OLD casing. Removing them lets the
# DelphiScript's lowercase filenames take effect.
Get-ChildItem -Path $BridgeDir -File -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '\.(PcbLib|SchLib)$' } | Remove-Item -Force

# Deterministic 2x2 24-bit BMP (70 bytes) for the EMBIMGSYM embedded-image symbol:
# the DelphiScript's AddImageEmbedded points Altium at this file so the saved
# SchLib carries real image bytes in its /Storage stream. A byte-identical copy
# is committed at scripts\samples\embed.bmp for the read tests to compare against.
$EmbedBmp = Join-Path $BridgeDir 'embed.bmp'
[byte[]]$bmpBytes = @(
    0x42, 0x4D, 0x46, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x36, 0x00, 0x00, 0x00, # 'BM', size 70, pixel offset 54
    0x28, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, # info header 40, 2x2, 1 plane
    0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x13, 0x0B, 0x00, 0x00, # 24 bpp, no compression, size-image 0 (BI_RGB; matches Altium's own normalisation)
    0x13, 0x0B, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,             # 2835 ppm, 0 palette colours
    0x00, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0x00, 0x00,                                     # bottom row: red, green + pad
    0xFF, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00, 0x00                                      # top row: blue, white + pad
)
[System.IO.File]::WriteAllBytes($EmbedBmp, $bmpBytes)

Write-Host "Authoring sample libraries..."

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
Write-Host "`nSample libraries written to $SamplesDir." -ForegroundColor Green

# Close Altium unless asked otherwise (the documents are already saved above).
# Loop-kill: Altium can take a moment to exit / spawn helper X2 processes.
if (-not $KeepAltiumOpen) {
    $deadline = (Get-Date).AddSeconds(15)
    while ((Get-Process X2 -ErrorAction SilentlyContinue) -and (Get-Date) -lt $deadline) {
        Get-Process X2 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 500
    }
    if (Get-Process X2 -ErrorAction SilentlyContinue) {
        Write-Host "Altium still running (could not fully close)." -ForegroundColor Yellow
    } else {
        Write-Host "Closed Altium." -ForegroundColor Green
    }
}
