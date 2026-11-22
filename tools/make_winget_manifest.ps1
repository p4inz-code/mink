#Requires -Version 5.1
<#
.SYNOPSIS
    Writes the three winget manifest files for a finished MINK setup file.

.DESCRIPTION
    Takes the mink-Setup-<version>.exe from tools\build_installer.ps1, reads its SHA-256 and writes
    the version, installer and locale manifests under packaging\winget\<version>\. The download address
    points at the GitHub release for that version, so upload the setup file there first, unchanged.

.PARAMETER Setup
    The setup file. Defaults to dist\installer\mink-Setup-<version>.exe.

.PARAMETER Version
    The version. Defaults to the one in Cargo.toml.

.EXAMPLE
    PS> .\tools\make_winget_manifest.ps1
#>
[CmdletBinding()]
param(
    [string]$Setup,
    [string]$Version
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (-not $Version) {
    $line = Select-String -Path (Join-Path $repo 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
    $Version = $line.Matches[0].Groups[1].Value
}
if (-not $Setup) { $Setup = Join-Path $repo "dist\installer\mink-Setup-$Version.exe" }
if (-not (Test-Path -LiteralPath $Setup)) { throw "The setup file was not found: $Setup. Run tools\build_installer.ps1 first." }
$hash = (Get-FileHash -LiteralPath $Setup -Algorithm SHA256).Hash.ToUpperInvariant()
$id = 'P4inz.MINK'
$url = "https://github.com/p4inz-code/mink/releases/download/v$Version/mink-Setup-$Version.exe"
$dir = Join-Path $repo "packaging\winget\$Version"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$schema = '1.6.0'

$versionYaml = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.version.$schema.schema.json
PackageIdentifier: $id
PackageVersion: $Version
DefaultLocale: en-US
ManifestType: version
ManifestVersion: $schema
"@

$installerYaml = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.installer.$schema.schema.json
PackageIdentifier: $id
PackageVersion: $Version
InstallerType: inno
Scope: user
InstallModes:
  - interactive
  - silent
InstallerSwitches:
  Silent: /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
  SilentWithProgress: /SILENT /SUPPRESSMSGBOXES /NORESTART
MinimumOSVersion: 10.0.0.0
Installers:
  - Architecture: x64
    InstallerUrl: $url
    InstallerSha256: $hash
ManifestType: installer
ManifestVersion: $schema
"@

$localeYaml = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.defaultLocale.$schema.schema.json
PackageIdentifier: $id
PackageVersion: $Version
PackageLocale: en-US
Publisher: P4inz Interactive Labs
PublisherUrl: https://github.com/p4inz-code
PackageName: MINK
PackageUrl: https://github.com/p4inz-code/mink
License: Apache-2.0
LicenseUrl: https://github.com/p4inz-code/mink/blob/main/LICENSE
Copyright: Copyright 2026 Atharva Patil | P4inz | P4inz Interactive Labs
ShortDescription: A programming language that builds native Windows programs from small, readable source files.
Tags:
  - compiler
  - programming-language
  - windows
ManifestType: defaultLocale
ManifestVersion: $schema
"@

Set-Content -LiteralPath (Join-Path $dir "$id.yaml") -Value $versionYaml -Encoding UTF8
Set-Content -LiteralPath (Join-Path $dir "$id.installer.yaml") -Value $installerYaml -Encoding UTF8
Set-Content -LiteralPath (Join-Path $dir "$id.locale.en-US.yaml") -Value $localeYaml -Encoding UTF8
Write-Host "Wrote winget manifests for MINK $Version in $dir"
Write-Host "Setup SHA-256: $hash"
Write-Host 'Upload the setup file to the GitHub release unchanged. The hash above must match what people download.'
