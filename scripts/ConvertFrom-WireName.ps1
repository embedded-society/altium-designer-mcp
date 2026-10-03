<#
.SYNOPSIS
    Shared helper: decode a component name from Altium's on-wire form.

.DESCRIPTION
    Altium's PCB scripting API hands back a non-Latin component name in its
    on-wire form: the UTF-8 bytes carried one char per byte. That is not a
    defect in the file — asking Altium for the names in its OWN authored golden
    returns exactly the same string — so name comparisons accept either the
    true name or that form. Decoding it back is the inverse of what the writer
    does, through the system ANSI page, because that is the one Altium widened
    through (see scripts/README.md § "Altium's PCB scripting API returns names
    in their on-wire form"). Altium hands back the broken bar it stores for a
    pipe (byte 0xA6 widened) as `|`, which no name holds, so that is mapped
    back first.
#>
function ConvertFrom-WireName([string]$Name) {
    try {
        $bytes = [System.Text.Encoding]::Default.GetBytes($Name.Replace('|', [string][char]0xA6))
        $utf8  = New-Object System.Text.UTF8Encoding $false, $true
        return $utf8.GetString($bytes)
    } catch { return $Name }
}
