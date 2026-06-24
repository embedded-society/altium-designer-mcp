<#
.SYNOPSIS
    On-site: verify that .PcbLib / .SchLib files open cleanly in a real Altium Designer.

.DESCRIPTION
    Writes the library paths to a bridge request file, launches Altium with the
    AltiumVerify DelphiScript via the RunScript CLI, polls for the JSON response, and
    reports PASS/FAIL per file. This is the ground-truth check that the pyaltiumlib
    oracle only approximates.

    The RunScript launch + file-based request/response bridge are adapted from
    coffeenmusic/altium-mcp (MIT) — https://github.com/coffeenmusic/altium-mcp

    On-site only: needs Altium Designer installed (developed against AD24). Never CI.

.PARAMETER Files
    One or more .PcbLib / .SchLib paths to verify.

.PARAMETER AltiumExe
    Path to X2.EXE. Read from scripts\.env.local (ALTIUM_EXE) when omitted.

.PARAMETER TimeoutSeconds
    How long to wait for Altium to write the response (default 180).

.EXAMPLE
    .\Verify-Libraries.ps1 -Files C:\tmp\Verify.PcbLib, C:\tmp\Verify.SchLib
#>
param(
    [Parameter(Mandatory = $true)][string[]]$Files,
    [string]$AltiumExe,
    [int]$TimeoutSeconds = 180
)

$ErrorActionPreference = 'Stop'

$BridgeDir    = 'C:\Users\Public\altium_designer_mcp'
$RequestFile  = Join-Path $BridgeDir 'verify_request.txt'
$ResponseFile = Join-Path $BridgeDir 'verify_response.json'
$ScriptDir    = Split-Path -Parent $MyInvocation.MyCommand.Path
$PrjScr       = Join-Path $ScriptDir 'altium\verify\AltiumVerify.PrjScr'

if (-not (Test-Path $PrjScr)) { throw "Verify project not found: $PrjScr" }

# 1. Resolve X2.EXE from .env.local at the repo root (no auto-discovery — multiple
#    Altium versions may be installed).
. (Join-Path $ScriptDir 'Resolve-AltiumExe.ps1')
$AltiumExe = Resolve-AltiumExe -Override $AltiumExe -EnvFile (Join-Path (Split-Path -Parent $ScriptDir) '.env.local')
Write-Host "Altium : $AltiumExe"

# 2. Resolve the library paths to absolute
$abs = foreach ($f in $Files) {
    if (-not (Test-Path $f)) { throw "File not found: $f" }
    (Resolve-Path $f).Path
}

# 3. Write the request; clear any stale response
New-Item -ItemType Directory -Force -Path $BridgeDir | Out-Null
# Write the paths without a BOM (a UTF-8 BOM would prefix the first path).
[System.IO.File]::WriteAllLines($RequestFile, [string[]]$abs)
if (Test-Path $ResponseFile) { Remove-Item $ResponseFile -Force }
Write-Host "Verifying $($abs.Count) file(s)..."

# 4. Launch Altium with the verify script. We write the exact cmd line (with the
#    `^|` separator that RunScript expects) to a .bat and run it — the most reliable
#    way to pass this argument's pipe/quotes through Windows. Matches the proven
#    invocation used by coffeenmusic/altium-mcp.
$bat = Join-Path $env:TEMP 'altium_designer_mcp_verify_launch.bat'
"`"$AltiumExe`" -RScriptingSystem:RunScript(ProjectName=`"$PrjScr`"^|ProcName=`"AltiumVerify>Run`")" |
    Set-Content -Path $bat -Encoding ASCII
Start-Process -FilePath $bat -WindowStyle Hidden

# 5. Poll for the response file
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
while (-not (Test-Path $ResponseFile) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 500 }
if (-not (Test-Path $ResponseFile)) {
    throw "Timed out after $TimeoutSeconds s waiting for Altium's response. " +
          "If a library is corrupt, Altium may be showing a modal 'catastrophic failure' dialog " +
          "(dismiss it and check the file)."
}

# 6. Report
$raw = Get-Content $ResponseFile -Raw
$results = $raw | ConvertFrom-Json
# An error response is a single JSON object {"error":...}; a success response is a
# JSON array. (Testing $results.error directly mis-fires on an array, because member
# enumeration returns one item per element.)
if ($raw.TrimStart([char]0xFEFF, ' ', "`t", "`r", "`n").StartsWith('{')) {
    throw "Altium verify script error: $($results.error)"
}

$allOk = $true
foreach ($r in @($results)) {
    if ($r.opened) {
        Write-Host ("  PASS  {0}" -f $r.file) -ForegroundColor Green
    } else {
        $allOk = $false
        Write-Host ("  FAIL  {0}  ({1})" -f $r.file, $r.detail) -ForegroundColor Red
    }
}

if ($allOk) {
    Write-Host "`nAll libraries opened in Altium." -ForegroundColor Green
} else {
    Write-Host "`nSome libraries FAILED to open." -ForegroundColor Red
    exit 1
}
