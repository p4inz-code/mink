#Requires -Version 5.1
<#
.SYNOPSIS
    Registers or removes the per-user Windows file association for `.mink` source files.

.DESCRIPTION
    Gives MINK source files their own Windows identity:

        .mink  ->  MINK.SourceFile  ->  "MINK Source File"  ->  mink-file.ico

    Everything is written under HKCU (per-user). No administrator rights are required and
    nothing under HKLM is touched.

    A `.mink` file is source code, so double-clicking it must never compile or run it.
    This script therefore does not register mink.exe as the open command. Without
    -Editor the file type is registered without an `open` verb: Windows uses its normal
    "Open with" flow, remembers the application the user picks, and MINK never executes
    the file. Pass -Editor <path-to-editor.exe> to bind one specific editor instead.

    The script is idempotent: running it repeatedly produces the same registry state.
    -Uninstall removes only the entries this script created; values that belong to
    another application, and the user's own "Open with" choice, are left alone.

.PARAMETER InstallDir
    MINK installation directory, either the directory holding `mink.exe` or its parent
    (a `bin` subdirectory is accepted). Used to find `mink.exe` and a `mink-file.ico`
    copy next to it. When omitted, the installation is resolved from this script's own
    location -- the npm package ships it in `<package>\windows`, beside
    `<package>\bin\mink.exe` -- and then from `mink.exe` on PATH.

.PARAMETER MinkExe
    Explicit path to `mink.exe`. Its directory is used as the installation directory.

.PARAMETER IconPath
    Explicit path to the `.mink` icon file (`.ico`). Defaults to `mink-file.ico`, first
    beside the installation, then beside this script.

.PARAMETER Editor
    Path to the editor executable that opens `.mink` files, for example
    "C:\Program Files\Microsoft VS Code\Code.exe". Omit it (recommended for a first
    install) to leave the choice to Windows.

.PARAMETER Force
    Replace a `.mink` association that another application currently owns. The previous
    owner is recorded and restored by -Uninstall. Another application's own values, and
    a class-level `OpenWithProgids` list it may have created, are never deleted -- while
    such a list exists Windows treats the extension as "open with" and asks the user to
    confirm the application once.

.PARAMETER Uninstall
    Remove the MINK-owned association instead of creating it.

.EXAMPLE
    PS> .\register-mink-filetype.ps1

.EXAMPLE
    PS> .\register-mink-filetype.ps1 -InstallDir "$env:LOCALAPPDATA\Programs\MINK"

.EXAMPLE
    PS> .\register-mink-filetype.ps1 -Editor "C:\Program Files\Microsoft VS Code\Code.exe"

.EXAMPLE
    PS> .\register-mink-filetype.ps1 -Uninstall

.NOTES
    Exit code 0 on success, 1 on refusal or error. See ..\README.md and
    docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md for the design, the validation procedure
    and the limitations.

    All registry access uses the .NET `Microsoft.Win32.Registry` API rather than the
    PowerShell registry provider. Three provider behaviours make it unsuitable here: it
    creates a key with `New-Item -Force` by discarding the existing key and its values
    (which would delete another application's entries), it cannot create missing parent
    keys, and `Remove-ItemProperty -Name '(default)'` fails on the unnamed default value.
#>
[CmdletBinding()]
param(
    [string]$InstallDir,
    [string]$MinkExe,
    [string]$IconPath,
    [string]$Editor,
    [switch]$Force,
    [switch]$Uninstall
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# --- Windows identity of the file type -------------------------------------
$Extension = '.mink'
$ProgId = 'MINK.SourceFile'
$TypeName = 'MINK Source File'
$ContentType = 'text/x-mink'
$PerceivedType = 'text'

# --- Registry locations, all per-user and all HKCU-relative ----------------
$ExtensionSubKey = 'Software\Classes\.mink'
$ProgIdSubKey = 'Software\Classes\MINK.SourceFile'
$DefaultIconSubKey = "$ProgIdSubKey\DefaultIcon"
$ShellSubKey = "$ProgIdSubKey\shell"
$OpenVerbSubKey = "$ShellSubKey\open"
$OpenCommandSubKey = "$OpenVerbSubKey\command"
$OpenWithSubKey = 'Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.mink\OpenWithProgids'
$OpenWithParentSubKey = Split-Path -Parent $OpenWithSubKey

# Remembers the ProgID that owned `.mink` before -Force replaced it, so -Uninstall can
# put it back. Lives inside the MINK ProgID tree, which -Uninstall removes.
$PreviousOwnerValue = 'PreviousDotMinkOwner'

$removed = 0
$notes = New-Object System.Collections.Generic.List[string]

function Stop-WithError {
    param([string]$Message)
    Write-Host "ERROR: $Message" -ForegroundColor Red
    exit 1
}

function Get-FullPath {
    param([string]$Path, [string]$What)
    if (-not (Test-Path -LiteralPath $Path)) {
        Stop-WithError "$What not found: $Path"
    }
    return (Resolve-Path -LiteralPath $Path).Path
}

# --- Registry helpers (HKCU-relative paths) --------------------------------
function Open-RegistryKey {
    param([string]$SubKey, [switch]$Writable)
    return [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($SubKey, [bool]$Writable)
}

function Test-RegistryKey {
    param([string]$SubKey)
    $key = Open-RegistryKey -SubKey $SubKey
    if (-not $key) { return $false }
    $key.Dispose()
    return $true
}

function Set-RegistryValue {
    param([string]$SubKey, [string]$Name, [string]$Value)
    # CreateSubKey opens an existing key without touching its values and creates missing
    # parents, so a plain write is safe and complete.
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($SubKey)
    if (-not $key) { Stop-WithError "Could not open HKCU:\$SubKey for writing." }
    try {
        # "" addresses the unnamed default value.
        $key.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]::String)
    }
    finally { $key.Dispose() }
}

function Read-RegistryValue {
    param([string]$SubKey, [string]$Name = '')
    $key = Open-RegistryKey -SubKey $SubKey
    if (-not $key) { return $null }
    try { return $key.GetValue($Name) }
    finally { $key.Dispose() }
}

function Remove-RegistryValue {
    param([string]$SubKey, [string]$Name = '')
    $key = Open-RegistryKey -SubKey $SubKey -Writable
    if (-not $key) { return $false }
    try {
        if (-not ($key.GetValueNames() -contains $Name)) { return $false }
        $key.DeleteValue($Name, $false)
        return $true
    }
    finally { $key.Dispose() }
}

function Remove-RegistryTree {
    param([string]$SubKey)
    [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($SubKey, $false)
}

function Remove-RegistryKeyIfEmpty {
    param([string]$SubKey)
    $key = Open-RegistryKey -SubKey $SubKey
    if (-not $key) { return }
    try {
        $empty = ($key.SubKeyCount -eq 0 -and $key.ValueCount -eq 0 -and -not $key.GetValue(''))
    }
    finally { $key.Dispose() }
    if ($empty) { Remove-RegistryTree -SubKey $SubKey }
}

# --- Installation and icon resolution --------------------------------------
function Resolve-InstallDirectory {
    param([string]$ExplicitDir, [string]$ExplicitExe)

    $candidates = New-Object System.Collections.Generic.List[string]
    if ($ExplicitExe) {
        $exePath = Get-FullPath -Path $ExplicitExe -What 'mink.exe'
        $candidates.Add((Split-Path -Parent $exePath))
    }
    if ($ExplicitDir) {
        $dir = Get-FullPath -Path $ExplicitDir -What 'installation directory'
        $candidates.Add($dir)
        $candidates.Add((Join-Path $dir 'bin'))
    }
    # npm package layout: this script is <package>\windows\register-mink-filetype.ps1.
    $packageRoot = Split-Path -Parent $PSScriptRoot
    $candidates.Add((Join-Path $packageRoot 'bin'))
    $candidates.Add($packageRoot)
    # `mink.exe` on PATH (the npm global shim directory, or a manual install).
    $onPath = Get-Command -Name mink -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($onPath) { $candidates.Add((Split-Path -Parent $onPath.Source)) }

    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path -LiteralPath (Join-Path $candidate 'mink.exe'))) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }
    return $null
}

function Resolve-FileIcon {
    param([string]$Explicit, [string]$ResolvedInstallDir)

    if ($Explicit) {
        return (Get-FullPath -Path $Explicit -What 'icon')
    }
    $candidates = New-Object System.Collections.Generic.List[string]
    if ($ResolvedInstallDir) { $candidates.Add((Join-Path $ResolvedInstallDir 'mink-file.ico')) }
    $candidates.Add((Join-Path $PSScriptRoot 'mink-file.ico'))
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }
    Stop-WithError ("No mink-file.ico found. Pass -IconPath, or put mink-file.ico next to " +
        "mink.exe in the installation directory.")
}

function Resolve-EditorPath {
    param([string]$Value)

    if (Test-Path -LiteralPath $Value -PathType Leaf) {
        return (Resolve-Path -LiteralPath $Value).Path
    }
    $command = Get-Command -Name $Value -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($command) { return $command.Source }
    Stop-WithError "Editor not found: $Value. Pass the full path to the editor executable."
}

# --- Shell notification ----------------------------------------------------
function Update-ShellAssociations {
    # SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, NULL, NULL) tells the shell that
    # associations changed. Defined once per session; adding the same type twice fails.
    if (-not ('Mink.ShellRefresh' -as [type])) {
        Add-Type -Namespace Mink -Name ShellRefresh -MemberDefinition @'
[DllImport("shell32.dll")]
public static extern void SHChangeNotify(int wEventId, uint uFlags, IntPtr dwItem1, IntPtr dwItem2);
'@
    }
    [Mink.ShellRefresh]::SHChangeNotify(0x08000000, 0x0000, [IntPtr]::Zero, [IntPtr]::Zero)

    # Refresh the cached file-type icon without restarting Explorer.
    $ie4uinit = Join-Path $env:SystemRoot 'System32\ie4uinit.exe'
    if (Test-Path -LiteralPath $ie4uinit) {
        try { & $ie4uinit -show | Out-Null }
        catch { Write-Warning "Icon cache refresh (ie4uinit.exe -show) failed: $($_.Exception.Message)" }
    }
}

function Write-PathState {
    param([string]$Label, [string]$Value)
    Write-Host ("  {0,-12}: {1}" -f $Label, $Value)
}

# --- Register --------------------------------------------------------------
function Register-MinkFileType {
    param([string]$ResolvedIcon, [string]$ResolvedEditor)

    $currentOwner = Read-RegistryValue -SubKey $ExtensionSubKey
    if ($currentOwner -and $currentOwner -ne $ProgId -and -not $Force) {
        Stop-WithError ("The extension $Extension is currently associated with '$currentOwner'. " +
            "Nothing was changed. Re-run with -Force to replace it, or remove that association first.")
    }

    Set-RegistryValue -SubKey $ExtensionSubKey -Name '' -Value $ProgId
    Set-RegistryValue -SubKey $ExtensionSubKey -Name 'Content Type' -Value $ContentType
    Set-RegistryValue -SubKey $ExtensionSubKey -Name 'PerceivedType' -Value $PerceivedType

    Set-RegistryValue -SubKey $ProgIdSubKey -Name '' -Value $TypeName
    Set-RegistryValue -SubKey $ProgIdSubKey -Name 'FriendlyTypeName' -Value $TypeName

    # Quoted because installation paths contain spaces; ",0" selects the first image.
    Set-RegistryValue -SubKey $DefaultIconSubKey -Name '' -Value ('"{0}",0' -f $ResolvedIcon)

    if ($ResolvedEditor) {
        Set-RegistryValue -SubKey $OpenCommandSubKey -Name '' -Value ('"{0}" "%1"' -f $ResolvedEditor)
    }
    elseif (Test-RegistryKey -SubKey $OpenCommandSubKey) {
        # A previous run bound an editor; with no -Editor the association must go back to
        # letting Windows choose, so the MINK-owned open command is removed.
        Remove-RegistryTree -SubKey $OpenCommandSubKey
        Remove-RegistryKeyIfEmpty -SubKey $OpenVerbSubKey
        Remove-RegistryKeyIfEmpty -SubKey $ShellSubKey
        $notes.Add('removed the previously bound editor command (-Editor not given)')
    }

    # Make the MINK file type appear in Explorer's "Open with" list. The value is
    # REG_NONE with no data, which is what the shell expects for a ProgID list.
    $openWith = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($OpenWithSubKey)
    if (-not $openWith) { Stop-WithError "Could not open HKCU:\$OpenWithSubKey for writing." }
    try {
        $openWith.SetValue($ProgId, [byte[]]@(), [Microsoft.Win32.RegistryValueKind]::None)
    }
    finally { $openWith.Dispose() }

    # Bookkeeping for -Uninstall: only the first takeover is remembered, so repeated
    # -Force runs cannot overwrite the original owner with 'MINK.SourceFile'.
    if ($currentOwner -and $currentOwner -ne $ProgId) {
        if (-not (Read-RegistryValue -SubKey $ProgIdSubKey -Name $PreviousOwnerValue)) {
            Set-RegistryValue -SubKey $ProgIdSubKey -Name $PreviousOwnerValue -Value $currentOwner
        }
    }
}

# --- Uninstall -------------------------------------------------------------
function Unregister-MinkFileType {
    $previousOwner = Read-RegistryValue -SubKey $ProgIdSubKey -Name $PreviousOwnerValue

    if (Test-RegistryKey -SubKey $ExtensionSubKey) {
        $owner = Read-RegistryValue -SubKey $ExtensionSubKey
        if ($owner -eq $ProgId) {
            if (Remove-RegistryValue -SubKey $ExtensionSubKey) { $script:removed++ }
            if ($previousOwner) {
                Set-RegistryValue -SubKey $ExtensionSubKey -Name '' -Value $previousOwner
                $notes.Add("restored the previous owner of $Extension ($previousOwner)")
            }
        }
        elseif ($owner) {
            $notes.Add("left $Extension associated with '$owner' (not created by MINK)")
        }
        if ((Read-RegistryValue -SubKey $ExtensionSubKey -Name 'Content Type') -eq $ContentType) {
            if (Remove-RegistryValue -SubKey $ExtensionSubKey -Name 'Content Type') { $script:removed++ }
        }
        if ((Read-RegistryValue -SubKey $ExtensionSubKey -Name 'PerceivedType') -eq $PerceivedType) {
            if (Remove-RegistryValue -SubKey $ExtensionSubKey -Name 'PerceivedType') { $script:removed++ }
        }
    }

    if (Test-RegistryKey -SubKey $ProgIdSubKey) {
        Remove-RegistryTree -SubKey $ProgIdSubKey
        $notes.Add("removed HKCU:\$ProgIdSubKey")
    }

    if (Test-RegistryKey -SubKey $OpenWithSubKey) {
        if (Remove-RegistryValue -SubKey $OpenWithSubKey -Name $ProgId) { $script:removed++ }
        Remove-RegistryKeyIfEmpty -SubKey $OpenWithSubKey
        Remove-RegistryKeyIfEmpty -SubKey $OpenWithParentSubKey
    }

    # Drop the extension key only if it is now completely empty; an extension that was
    # already in use keeps whatever the other application put there.
    Remove-RegistryKeyIfEmpty -SubKey $ExtensionSubKey

    $notes.Add('the per-user "Open with" choice for .mink is user-owned and was not touched')
}

# --- Main ------------------------------------------------------------------
Write-Host 'MINK file association (.mink)'
Write-Host ''

if ($Uninstall) {
    Unregister-MinkFileType
    Update-ShellAssociations
    Write-Host 'State'
    Write-PathState -Label 'extension' -Value "$Extension is no longer owned by $ProgId"
    Write-PathState -Label 'prog id' -Value "HKCU:\$ProgIdSubKey removed"
    Write-PathState -Label 'removed' -Value "$removed MINK-owned value(s)"
    Write-PathState -Label 'explorer' -Value 'association change broadcast, icon cache refreshed'
    Write-Host ''
    foreach ($note in $notes) { Write-Host "  note        : $note" }
    Write-Host ''
    Write-Host 'Done. MINK-owned file-association entries were removed.'
    exit 0
}

$resolvedInstallDir = Resolve-InstallDirectory -ExplicitDir $InstallDir -ExplicitExe $MinkExe
$resolvedIcon = Resolve-FileIcon -Explicit $IconPath -ResolvedInstallDir $resolvedInstallDir
$resolvedEditor = $null
if ($Editor) {
    $resolvedEditor = Resolve-EditorPath -Value $Editor
    if ([System.IO.Path]::GetExtension($resolvedEditor) -ne '.exe') {
        Write-Warning ("The editor '$resolvedEditor' is not an .exe; Windows opens files with it " +
            "through a script shim. Pass the real executable when one exists.")
    }
}

Register-MinkFileType -ResolvedIcon $resolvedIcon -ResolvedEditor $resolvedEditor
Update-ShellAssociations

Write-Host 'State'
Write-PathState -Label 'file type' -Value "$TypeName ($Extension, $ContentType)"
Write-PathState -Label 'prog id' -Value $ProgId
if ($resolvedInstallDir) {
    Write-PathState -Label 'install dir' -Value "$resolvedInstallDir (mink.exe found)"
}
else {
    Write-PathState -Label 'install dir' -Value 'not resolved; only the icon is needed'
}
Write-PathState -Label 'icon' -Value $resolvedIcon
if ($resolvedEditor) {
    Write-PathState -Label 'open with' -Value $resolvedEditor
}
else {
    Write-PathState -Label 'open with' -Value 'Windows "Open with"; no command registered, source is never executed'
}
Write-PathState -Label 'registry' -Value "HKCU:\$ExtensionSubKey -> $ProgId (per-user)"
Write-PathState -Label 'explorer' -Value 'association change broadcast, icon cache refreshed'
Write-Host ''
foreach ($note in $notes) { Write-Host "  note        : $note" }
Write-Host ''
if ($resolvedEditor) {
    Write-Host 'Done. Double-clicking a .mink file opens it in the registered editor.'
}
else {
    Write-Host 'Done. Double-clicking a .mink file asks Windows which application to use the first'
    Write-Host 'time and remembers that choice; the source is never compiled or run.'
}
exit 0
