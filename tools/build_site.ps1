#Requires -Version 5.1
<#
.SYNOPSIS
    Builds the MINK website with a MINK program (testapps\sitegen).

.DESCRIPTION
    Compiles testapps\sitegen with the given mink.exe, then fills every site\src\*.tpl.html from
    site\src\site.data and writes the page next to it in site\. Run tools\update_site_data.ps1 first when
    the release or the benchmarks changed. Run it from anywhere.

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
    foreach ($tpl in Get-ChildItem site\src -Filter *.tpl.html) {
        $name = $tpl.Name -replace '\.tpl\.html$', '.html'
        & .\testapps\sitegen\main.exe "site\src\$($tpl.Name)" site\src\site.data "site\$name" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "sitegen failed on $($tpl.Name)" }
        Write-Host "built site\$name"
    }
}
finally { Pop-Location }
