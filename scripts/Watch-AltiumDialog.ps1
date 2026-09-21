<#
.SYNOPSIS
    Watches a headless Altium generate run for the error dialog that ends it.

.DESCRIPTION
    A DelphiScript compile error opens a modal dialog naming the offending
    identifier, and a few properties take AD24 down with a native access
    violation instead. Headless, either just sits there until
    Generate-Samples.ps1 times out, so the run costs seven minutes and reveals
    nothing about which name was wrong.

    This polls X2's windows for that dialog, prints its text, and force-kills
    Altium — turning a silent seven-minute failure into a named ~30-second one.
    That is what makes it safe to put several unproven identifiers in one run:
    a failure identifies itself instead of leaving the whole batch ambiguous.

    Run it alongside the generator, not instead of it:

        $gen = Start-Process powershell -PassThru -WindowStyle Hidden -ArgumentList @(
            '-ExecutionPolicy','Bypass','-File','scripts\Generate-Samples.ps1')
        & powershell -ExecutionPolicy Bypass -File scripts\Watch-AltiumDialog.ps1
        # exit 0 = the run finished, 2 = a dialog was caught, 3 = timed out silently

    Check names offline first with scripts/altium/generate/preflight_names.py;
    this catches only what survives that.

.PARAMETER TimeoutSeconds
    How long to watch before giving up (default 420, matching the generator).

.PARAMETER ResponseFile
    The file whose appearance means the run finished (default: the generator's
    generate_response.json). A probe script that writes its own response names it
    here; the caller deletes a stale copy before launching Altium.

.NOTES
    On-site only: needs Altium installed. Never CI.
#>
param(
    [int]$TimeoutSeconds = 420,
    [string]$ResponseFile = 'C:\Users\Public\altium_designer_mcp\samples\generate_response.json'
)

Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Win {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr h, EnumProc cb, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public static string Text(IntPtr h) {
    var sb = new StringBuilder(1024);
    GetWindowTextW(h, sb, sb.Capacity);
    return sb.ToString();
  }
}
'@

function Get-X2WindowText {
    $pids = @(Get-Process X2 -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
    if (-not $pids) { return @() }
    $texts = New-Object System.Collections.ArrayList
    $collectChild = [Win+EnumProc] {
        param($h, $l)
        $t = [Win]::Text($h)
        if ($t) { [void]$texts.Add($t) }
        return $true
    }
    $collectTop = [Win+EnumProc] {
        param($h, $l)
        $procId = 0
        [void][Win]::GetWindowThreadProcessId($h, [ref]$procId)
        if ($pids -contains [int]$procId -and [Win]::IsWindowVisible($h)) {
            $t = [Win]::Text($h)
            if ($t) { [void]$texts.Add($t) }
            [void][Win]::EnumChildWindows($h, $collectChild, [IntPtr]::Zero)
        }
        return $true
    }
    [void][Win]::EnumWindows($collectTop, [IntPtr]::Zero)
    return $texts.ToArray()
}

$response = $ResponseFile
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)

while ((Get-Date) -lt $deadline) {
    if (Test-Path $response) { Write-Output 'RESULT: response file appeared (run finished)'; exit 0 }
    foreach ($t in Get-X2WindowText) {
        if ($t -match 'Undeclared identifier|Compiler|Syntax error|Unknown identifier|Access violation') {
            Write-Output "COMPILE-ERROR: $t"
            Get-Process X2 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
            exit 2
        }
    }
    Start-Sleep -Milliseconds 1500
}
Write-Output 'RESULT: timed out with no dialog matched'
exit 3
