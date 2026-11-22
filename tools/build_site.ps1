#Requires -Version 5.1
<#
.SYNOPSIS
    Builds the MINK website with a MINK program (testapps\sitegen).

.DESCRIPTION
    Compiles testapps\sitegen with the given mink.exe, then fills site\src\index.tpl.html from
    site\src\site.data and writes site\index.html. Run it from anywhere.

.PARAMETER Mink
    The mink.exe to use. Defaults to target\release\mink.exe.
#>
[CmdletBinding()]
param([string]$Mink)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (-not $Mink) { $Mink = Join-Path $repo 'target\release\mink.exe' }
Push-Location $repo
try {
    & $Mink build testapps\sitegen\main.mink | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'sitegen did not build' }
    & .\testapps\sitegen\main.exe site\src\index.tpl.html site\src\site.data site\index.html
    if ($LASTEXITCODE -ne 0) { throw 'sitegen failed' }
}
finally { Pop-Location }
