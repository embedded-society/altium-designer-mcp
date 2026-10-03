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
    Path to X2.EXE. Read from the repo-root .env.local (ALTIUM_EXE) when omitted.

.PARAMETER TimeoutSeconds
    How long to wait for Altium to write the response (default 180).

.PARAMETER Expect
    Optional path to a JSON expectations file: an array of objects with a `file`
    (matched by file name), and any of `components` (compared as a set — Altium
    iterates a library in its own shortlex order, not file order) and
    `primitive_counts` (array aligned with the entry's own `components`, which
    it therefore requires; matched to Altium's view by component name, and
    every key given must equal what Altium resolved — omit keys to leave them
    unasserted). The run fails if Altium's view differs, so a verify run can
    assert primitive counts and specific properties, not just "opened".

.EXAMPLE
    .\Verify-Libraries.ps1 -Files C:\tmp\Verify.PcbLib, C:\tmp\Verify.SchLib
#>
param(
    [Parameter(Mandatory = $true)][string[]]$Files,
    [string]$AltiumExe,
    [int]$TimeoutSeconds = 180,
    [string]$Expect
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

# Emit the parsed results so a caller can assert on more than PASS/FAIL — the
# per-file `components` array is what Verify-RoundTrip.ps1 compares against.
# Progress above goes through Write-Host, so this is the only pipeline output.
@($results)

if (-not $allOk) {
    Write-Host "`nSome libraries FAILED to open." -ForegroundColor Red
    exit 1
}
Write-Host "`nAll libraries opened in Altium." -ForegroundColor Green

# 7. Optional expectations: assert what Altium resolved, not just that it opened.
if ($Expect) {
    if (-not (Test-Path $Expect)) { throw "Expectations file not found: $Expect" }
    . (Join-Path $ScriptDir 'ConvertFrom-WireName.ps1')
    $mismatches = New-Object System.Collections.Generic.List[string]
    # Explicit UTF-8: Windows PowerShell would read a BOM-less file as ANSI and
    # mangle any non-Latin expected component name.
    $expected = [System.IO.File]::ReadAllText((Resolve-Path $Expect).Path, [System.Text.Encoding]::UTF8) |
        ConvertFrom-Json
    foreach ($e in @($expected)) {
        $leaf = Split-Path $e.file -Leaf
        $r = @($results) | Where-Object { (Split-Path $_.file -Leaf) -eq $leaf } | Select-Object -First 1
        if (-not $r) { $mismatches.Add("${leaf}: no verify result"); continue }
        $wantNames = @($e.components)
        # A non-Latin name comes back in Altium's on-wire form; accept either.
        $gotNames = @(@($r.components) | ForEach-Object {
            if ($wantNames -contains $_) { $_ } else { ConvertFrom-WireName $_ }
        })
        if ($e.PSObject.Properties.Name -contains 'components') {
            # A set comparison: Altium iterates a library in shortlex order
            # (name length, then alphabetical), not file order.
            $missing = @($wantNames | Where-Object { $gotNames -notcontains $_ })
            $extra   = @($gotNames  | Where-Object { $wantNames -notcontains $_ })
            if ($missing.Count -or $extra.Count) {
                $mismatches.Add("${leaf}: components missing [$($missing -join ', ')] unexpected [$($extra -join ', ')]")
            }
        }
        if ($e.PSObject.Properties.Name -contains 'primitive_counts') {
            $want = @($e.primitive_counts); $got = @($r.primitive_counts)
            if ($want.Count -ne $wantNames.Count) {
                $mismatches.Add("${leaf}: $($want.Count) count entries for $($wantNames.Count) components - primitive_counts must align with the entry's components")
                continue
            }
            # Matched by name: the two sides iterate in different orders.
            for ($i = 0; $i -lt $want.Count; $i++) {
                $name = $wantNames[$i]
                $j = [Array]::IndexOf($gotNames, $name)
                if ($j -lt 0) { continue }  # already reported as missing
                foreach ($p in $want[$i].PSObject.Properties) {
                    $actual = $got[$j].PSObject.Properties[$p.Name]
                    if (-not $actual) {
                        $mismatches.Add("${leaf} ${name}: Altium reported no '$($p.Name)' count")
                    } elseif ([int]$actual.Value -ne [int]$p.Value) {
                        $mismatches.Add("${leaf} ${name}: $($p.Name) expected $($p.Value), Altium resolved $($actual.Value)")
                    }
                }
            }
        }
    }
    if ($mismatches.Count) {
        Write-Host "`nEXPECTATIONS FAILED ($($mismatches.Count)):" -ForegroundColor Red
        $mismatches | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
        exit 1
    }
    Write-Host "Expectations met: Altium resolved the asserted components and primitive counts." -ForegroundColor Green
}
