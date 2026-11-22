#Requires -Version 5.1
<#
.SYNOPSIS
    Builds the MINK release files for Windows: the setup program, the portable zip and their checksums.

.DESCRIPTION
    Starting from a finished mink.exe, this produces under dist\ (or -Out):

        payload\                       what the installer installs: bin\mink.exe, stdlib\, windows\,
                                       LICENSE and payload-manifest.txt (a SHA-256 digest of every file)
        payload.zip                    the payload, for the setup wrapper
        mink-<version>-portable.zip    the same files for people who prefer to unzip and run
        installer\mink-Setup-<version>.exe
                                       the setup program (Inno Setup wrapper around payload.zip)
        *.sha256                       the SHA-256 of each file above, one per file

    mink.exe gets its icon and version information here, using `mink resource`, so the file in the
    zips is the one with the MINK icon. The setup program is not signed yet.

.PARAMETER Exe
    The mink.exe to ship. Defaults to target\release\mink.exe.

.PARAMETER Version
    The version to stamp. Defaults to the one in Cargo.toml.

.PARAMETER Out
    Where to write. Defaults to dist\ in the repository.

.PARAMETER SkipSetup
    Build the payload and the portable zip only (no Inno Setup needed).

.EXAMPLE
    PS> .\tools\build_installer.ps1 -Exe target\release\mink.exe

.NOTES
    Needs Windows PowerShell 5.1, the tar.exe that ships with Windows 10, and, unless -SkipSetup is
    given, Inno Setup 6 (ISCC.exe).
#>
[CmdletBinding()]
param(
    [string]$Exe,
    [string]$Version,
    [string]$Out,
    [switch]$SkipSetup
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
if (-not $Out) { $Out = Join-Path $repo 'dist' }
if (-not $Exe) { $Exe = Join-Path $repo 'target\release\mink.exe' }
if (-not (Test-Path -LiteralPath $Exe)) { throw "mink.exe not found: $Exe" }
$Exe = (Resolve-Path -LiteralPath $Exe).Path
if (-not $Version) {
    $cargo = Get-Content -LiteralPath (Join-Path $repo 'Cargo.toml') -Raw
    if ($cargo -match '(?m)^version\s*=\s*"([^"]+)"') { $Version = $Matches[1] } else { throw 'No version in Cargo.toml.' }
}
$reported = (& $Exe --version) -join ''
if ($reported -ne "mink $Version") { throw "The compiler reports '$reported', not 'mink $Version'. Rebuild it first." }

function Write-Checksum {
    param([string]$Path)
    $hash = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    $line = "$hash  $(Split-Path -Leaf $Path)"
    [System.IO.File]::WriteAllText("$Path.sha256", "$line`n", (New-Object System.Text.UTF8Encoding($false)))
    return $hash
}

$tar = Join-Path $env:SystemRoot 'System32\tar.exe'
if (-not (Test-Path -LiteralPath $tar)) { throw 'tar.exe was not found; Windows 10 1803 or later is needed.' }

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path -LiteralPath $Out).Path
$payload = Join-Path $Out 'payload'
if (Test-Path -LiteralPath $payload) { Remove-Item -LiteralPath $payload -Recurse -Force }
New-Item -ItemType Directory -Force -Path (Join-Path $payload 'bin') | Out-Null

# --- the files ---------------------------------------------------------------
Copy-Item -LiteralPath $Exe -Destination (Join-Path $payload 'bin\mink.exe')
Copy-Item -LiteralPath (Join-Path $repo 'stdlib') -Destination (Join-Path $payload 'stdlib') -Recurse
Copy-Item -LiteralPath (Join-Path $repo 'npm\mink\windows') -Destination (Join-Path $payload 'windows') -Recurse
Remove-Item -LiteralPath (Join-Path $payload 'windows\README.md') -Force -ErrorAction SilentlyContinue
Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination (Join-Path $payload 'LICENSE')
Copy-Item -LiteralPath (Join-Path $repo 'NOTICE') -Destination (Join-Path $payload 'NOTICE')

# --- the icon and version information, added by a copy of the compiler ----------
$driver = Join-Path $env:TEMP ("mink-driver-" + [guid]::NewGuid().ToString('N') + '.exe')
Copy-Item -LiteralPath $Exe -Destination $driver
try {
    & $driver resource (Join-Path $payload 'bin\mink.exe') `
        --icon (Join-Path $payload 'windows\mink-app.ico') `
        --product 'MINK' --version $Version `
        --description 'MINK compiler' --company 'P4inz Interactive Labs' `
        --copyright 'Copyright (C) 2026 Atharva Patil. Apache License 2.0.'
    if ($LASTEXITCODE -ne 0) { throw "mink resource failed with exit code $LASTEXITCODE" }
    & $driver installer --make-manifest $payload --version $Version
    if ($LASTEXITCODE -ne 0) { throw "writing the manifest failed with exit code $LASTEXITCODE" }
}
finally { Remove-Item -LiteralPath $driver -Force -ErrorAction SilentlyContinue }

# --- payload.zip --------------------------------------------------------------
$payloadZip = Join-Path $Out 'payload.zip'
if (Test-Path -LiteralPath $payloadZip) { Remove-Item -LiteralPath $payloadZip -Force }
& $tar -a -c -f $payloadZip -C $payload .
if ($LASTEXITCODE -ne 0) { throw 'tar.exe could not write payload.zip.' }

# --- portable zip ---------------------------------------------------------------
$portableName = "mink-$Version-portable"
$portableDir = Join-Path $Out $portableName
if (Test-Path -LiteralPath $portableDir) { Remove-Item -LiteralPath $portableDir -Recurse -Force }
Copy-Item -LiteralPath $payload -Destination $portableDir -Recurse
Remove-Item -LiteralPath (Join-Path $portableDir 'payload-manifest.txt') -Force
$readme = @"
MINK $Version, portable

This folder is MINK, ready to use. Nothing was installed and nothing on your PC was changed.

  1. Open a terminal in this folder and run:   bin\mink.exe --version
  2. To type just "mink" anywhere, run:        bin\mink.exe setup
     (it can add this folder to your PATH and give .mink files their icon; "mink setup --remove" undoes it)
  3. To try it:  bin\mink.exe run <your file>.mink

Keep the bin, stdlib and windows folders together: mink.exe finds the standard library and the
Windows kit next to itself. If you move the folder, run "mink setup" again.

Project: https://github.com/p4inz-code/mink
This build is not code-signed yet, so Windows SmartScreen may ask you to confirm it.
"@
[System.IO.File]::WriteAllText((Join-Path $portableDir 'README.txt'), $readme.Replace("`r`n", "`n"), (New-Object System.Text.UTF8Encoding($false)))
$portableZip = Join-Path $Out "$portableName.zip"
if (Test-Path -LiteralPath $portableZip) { Remove-Item -LiteralPath $portableZip -Force }
& $tar -a -c -f $portableZip -C $Out $portableName
if ($LASTEXITCODE -ne 0) { throw 'tar.exe could not write the portable zip.' }
Remove-Item -LiteralPath $portableDir -Recurse -Force

# --- the setup program ---------------------------------------------------------------
$setup = $null
if (-not $SkipSetup) {
    $iscc = @(
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe')
    ) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    if (-not $iscc) { throw 'Inno Setup 6 (ISCC.exe) was not found. Install it, or pass -SkipSetup.' }
    $installerDir = Join-Path $Out 'installer'
    New-Item -ItemType Directory -Force -Path $installerDir | Out-Null
    & $iscc "/DAppVer=$Version" "/DPayloadZip=$payloadZip" "/O$installerDir" (Join-Path $repo 'packaging\installer.iss')
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed with exit code $LASTEXITCODE" }
    $setup = Join-Path $installerDir "mink-Setup-$Version.exe"
    if (-not (Test-Path -LiteralPath $setup)) { throw "Inno Setup did not produce $setup" }
}

# --- checksums ----------------------------------------------------------------------
Write-Host ''
Write-Host "MINK $Version release files in $Out"
foreach ($file in @($payloadZip, $portableZip, $setup) | Where-Object { $_ }) {
    $hash = Write-Checksum -Path $file
    $size = (Get-Item -LiteralPath $file).Length.ToString('N0', [System.Globalization.CultureInfo]::InvariantCulture)
    Write-Host ('  {0,-34} {1,12} bytes  sha256 {2}' -f (Split-Path -Leaf $file), $size, $hash)
}
Write-Host ''
Write-Host 'Not code-signed yet: Windows SmartScreen may ask you to confirm the setup file.'
