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
    beside the installation, then beside this script. The file must be a readable ICO;
    registration refuses and changes nothing if it is missing, empty or malformed.

.PARAMETER Editor
    Path to the editor executable that opens `.mink` files, for example
    "C:\Program Files\Microsoft VS Code\Code.exe". Omit it (recommended for a first
    install) to leave the choice to Windows.

.PARAMETER Verbs
    Which right-click entries to add to `.mink` files: `run` ("Run with MINK"), `check`
    ("Check with MINK"), `build` ("Build executable") and `edit` ("Edit"). All four by
    default. Each runs `mink.exe shell <verb> "<file>"`, which keeps the console window open
    so the output can be read. `edit` opens the file in -Editor, or in Notepad when none is
    given. Entries that are not listed are removed if an earlier run added them.

.PARAMETER NoContextMenu
    Add no right-click entries at all (and remove the ones an earlier run added). Whenever
    any entry exists, "Edit" is among them: it is the default for double-click, so a
    double-click opens the file in an editor and never runs or builds it.

.PARAMETER NoNewMenu
    Do not add "MINK Source File" to Explorer's New menu.

.PARAMETER NoDefaultApps
    Do not list MINK under Settings > Apps > Default apps.

.PARAMETER NoPreview
    Do not register `.mink` as plain text for the Explorer preview pane and Windows Search.

.PARAMETER Quiet
    Print nothing except errors. The exit code still reports the result.

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
    [ValidateSet('run', 'check', 'build', 'edit')]
    [string[]]$Verbs = @('run', 'check', 'build', 'edit'),
    [switch]$NoContextMenu,
    [switch]$NoNewMenu,
    [switch]$NoDefaultApps,
    [switch]$NoPreview,
    [switch]$Quiet,
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

# Facilities beyond the icon, all per-user. Each one is recorded in the `MinkOwned`
# value of the ProgID key so -Uninstall removes exactly what this script added.
$NewMenuSubKey = "$ExtensionSubKey\ShellNew"
$PersistentHandlerSubKey = "$ExtensionSubKey\PersistentHandler"
$PreviewHandlerKeyName = '{8895b1c6-b41f-4c1c-a562-0d564250836f}'
$PreviewSubKey = "$ExtensionSubKey\shellex\$PreviewHandlerKeyName"
$PlainTextFilter = '{5e941d80-bf96-11cd-b579-08002b30bfeb}'
$CapabilitiesSubKey = 'Software\MINK\Capabilities'
$CapabilitiesParentSubKey = 'Software\MINK'
$RegisteredAppsSubKey = 'Software\RegisteredApplications'
$RegisteredAppName = 'MINK'
$OwnedValue = 'MinkOwned'
$VerbLabels = [ordered]@{
    run   = 'Run with MINK'
    check = 'Check with MINK'
    build = 'Build executable'
    edit  = 'Edit'
}

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

function Test-IconFile {
    param([string]$Path)

    # A path that exists is not enough: a truncated, empty or replaced file would
    # register a DefaultIcon the shell cannot draw, and Explorer would silently fall
    # back to a generic icon. Check the ICONDIR header (reserved 0, type 1, then the
    # frame count) before any registry state is touched.
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    try { $bytes = [System.IO.File]::ReadAllBytes($Path) }
    catch { return $false }
    if ($bytes.Length -lt 6) { return $false }
    if ($bytes[0] -ne 0 -or $bytes[1] -ne 0 -or $bytes[2] -ne 1 -or $bytes[3] -ne 0) { return $false }
    return (([int]$bytes[4] + 256 * [int]$bytes[5]) -gt 0)
}

function Resolve-FileIcon {
    param([string]$Explicit, [string]$ResolvedInstallDir)

    if ($Explicit) {
        $explicitPath = Get-FullPath -Path $Explicit -What 'icon'
        if (-not (Test-IconFile -Path $explicitPath)) {
            Stop-WithError ("-IconPath is not a valid .ico file: $explicitPath. " +
                "Pass the mink-file.ico shipped with MINK. Nothing was changed.")
        }
        return $explicitPath
    }

    # Resolution order: the installation directory (a manual install may keep the icon
    # next to mink.exe), then this script's own directory, which is where the npm
    # package ships it (<package>\windows). Both are derived from the running copy, so
    # no development path and no current working directory is ever assumed.
    $candidates = New-Object System.Collections.Generic.List[string]
    if ($ResolvedInstallDir) { $candidates.Add((Join-Path $ResolvedInstallDir 'mink-file.ico')) }
    $candidates.Add((Join-Path $PSScriptRoot 'mink-file.ico'))

    $present = New-Object System.Collections.Generic.List[string]
    foreach ($candidate in $candidates) {
        if (Test-IconFile -Path $candidate) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
        if (Test-Path -LiteralPath $candidate) { $present.Add($candidate) }
    }

    if ($present.Count -gt 0) {
        Stop-WithError (("Found an icon file that is not a valid .ico: {0}. " -f ($present -join ', ')) +
            "Reinstall MINK or pass a valid -IconPath. Nothing was changed.")
    }
    Stop-WithError (("No mink-file.ico found. Searched: {0}. Pass -IconPath, or put " -f ($candidates -join ', ')) +
        "mink-file.ico next to mink.exe in the installation directory. Nothing was changed.")
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
    if ($Quiet) { return }
    Write-Host ("  {0,-12}: {1}" -f $Label, $Value)
}

# --- Facilities beyond the icon --------------------------------------------
function Read-ClassesRootValue {
    param([string]$SubKey)
    # HKEY_CLASSES_ROOT is the merged per-user and per-machine view, which is where
    # Windows keeps the handlers registered for plain text.
    $key = [Microsoft.Win32.Registry]::ClassesRoot.OpenSubKey($SubKey, $false)
    if (-not $key) { return $null }
    try { return $key.GetValue('') }
    finally { $key.Dispose() }
}

function Resolve-AppIcon {
    param([string]$ResolvedInstallDir)
    $candidates = New-Object System.Collections.Generic.List[string]
    if ($ResolvedInstallDir) { $candidates.Add((Join-Path $ResolvedInstallDir 'mink-app.ico')) }
    $candidates.Add((Join-Path $PSScriptRoot 'mink-app.ico'))
    foreach ($candidate in $candidates) {
        if (Test-IconFile -Path $candidate) { return (Resolve-Path -LiteralPath $candidate).Path }
    }
    return $null
}

function Get-VerbCommand {
    param([string]$Verb, [string]$ResolvedMinkExe, [string]$ResolvedEditor)
    if ($Verb -eq 'edit') {
        $target = $ResolvedEditor
        if (-not $target) { $target = Join-Path $env:SystemRoot 'notepad.exe' }
        return ('"{0}" "%1"' -f $target)
    }
    return ('"{0}" shell {1} "%1"' -f $ResolvedMinkExe, $Verb)
}

function Register-ContextMenu {
    param([string]$ResolvedMinkExe, [string]$ResolvedEditor, [string]$AppIcon, [string[]]$Wanted)

    foreach ($verb in $VerbLabels.Keys) {
        $verbKey = "$ShellSubKey\$verb"
        if ($Wanted -contains $verb) {
            if ($verb -ne 'edit' -and -not $ResolvedMinkExe) {
                $notes.Add("skipped the '$verb' right-click entry: mink.exe was not found")
                continue
            }
            Set-RegistryValue -SubKey $verbKey -Name '' -Value $VerbLabels[$verb]
            if ($AppIcon -and $verb -ne 'edit') {
                Set-RegistryValue -SubKey $verbKey -Name 'Icon' -Value ('"{0}",0' -f $AppIcon)
            }
            if ($verb -ne 'edit') {
                # One program at a time: selecting many files must not start many consoles.
                Set-RegistryValue -SubKey $verbKey -Name 'MultiSelectModel' -Value 'Single'
            }
            Set-RegistryValue -SubKey "$verbKey\command" -Name '' -Value (
                Get-VerbCommand -Verb $verb -ResolvedMinkExe $ResolvedMinkExe -ResolvedEditor $ResolvedEditor)
        }
        elseif (Test-RegistryKey -SubKey $verbKey) {
            Remove-RegistryTree -SubKey $verbKey
            $notes.Add("removed the '$verb' right-click entry added by an earlier run")
        }
    }
}

function Test-Owned {
    param($Owned, [string]$Token)
    return $Owned.Contains($Token)
}

function Register-Facilities {
    param($Owned)

    # New menu: a template copied into each new file, or an empty file without one.
    if (-not $NoNewMenu) {
        if ((Test-RegistryKey -SubKey $NewMenuSubKey) -and -not (Test-Owned $Owned 'NewMenu')) {
            $notes.Add('left the existing New-menu entry for .mink alone (not created by MINK)')
        }
        else {
            Remove-RegistryTree -SubKey $NewMenuSubKey
            $template = Join-Path $PSScriptRoot 'template.mink'
            if (Test-Path -LiteralPath $template -PathType Leaf) {
                Set-RegistryValue -SubKey $NewMenuSubKey -Name 'FileName' -Value (Resolve-Path -LiteralPath $template).Path
            }
            else {
                Set-RegistryValue -SubKey $NewMenuSubKey -Name 'NullFile' -Value ''
                $notes.Add('template.mink not found; the New menu creates an empty file')
            }
            [void]$Owned.Add('NewMenu')
        }
    }
    elseif (Test-Owned $Owned 'NewMenu') {
        Remove-RegistryTree -SubKey $NewMenuSubKey
        [void]$Owned.Remove('NewMenu')
        $notes.Add('removed the New-menu entry added by an earlier run')
    }

    # Preview pane and Windows Search treat .mink as plain text, the way .txt is treated.
    if (-not $NoPreview) {
        if ((Test-RegistryKey -SubKey $PersistentHandlerSubKey) -and -not (Test-Owned $Owned 'PersistentHandler')) {
            $notes.Add('left the existing search filter for .mink alone (not created by MINK)')
        }
        else {
            Set-RegistryValue -SubKey $PersistentHandlerSubKey -Name '' -Value $PlainTextFilter
            [void]$Owned.Add('PersistentHandler')
        }
        if ((Test-RegistryKey -SubKey $PreviewSubKey) -and -not (Test-Owned $Owned 'PreviewHandler')) {
            $notes.Add('left the existing preview handler for .mink alone (not created by MINK)')
        }
        else {
            # Copy whatever handler this Windows version uses for .txt rather than
            # hard-coding a class id that differs between releases.
            $handler = Read-ClassesRootValue -SubKey ".txt\shellex\$PreviewHandlerKeyName"
            if ($handler) {
                Set-RegistryValue -SubKey $PreviewSubKey -Name '' -Value $handler
                [void]$Owned.Add('PreviewHandler')
            }
            else {
                $notes.Add('no text preview handler found for .txt on this PC; the preview pane is not enabled')
            }
        }
    }
    else {
        Remove-OwnedPreview -Owned $Owned
    }

    # "MINK" in Settings > Apps > Default apps.
    if (-not $NoDefaultApps) {
        Set-RegistryValue -SubKey $CapabilitiesSubKey -Name 'ApplicationName' -Value 'MINK'
        Set-RegistryValue -SubKey $CapabilitiesSubKey -Name 'ApplicationDescription' -Value 'The MINK programming language'
        Set-RegistryValue -SubKey "$CapabilitiesSubKey\FileAssociations" -Name $Extension -Value $ProgId
        Set-RegistryValue -SubKey $RegisteredAppsSubKey -Name $RegisteredAppName -Value $CapabilitiesSubKey
    }
    else {
        Remove-DefaultAppsEntry
    }
}

function Remove-OwnedPreview {
    param($Owned)
    if (Test-Owned $Owned 'PersistentHandler') {
        Remove-RegistryTree -SubKey $PersistentHandlerSubKey
        [void]$Owned.Remove('PersistentHandler')
    }
    if (Test-Owned $Owned 'PreviewHandler') {
        Remove-RegistryTree -SubKey $PreviewSubKey
        Remove-RegistryKeyIfEmpty -SubKey (Split-Path -Parent $PreviewSubKey)
        [void]$Owned.Remove('PreviewHandler')
    }
}

function Remove-DefaultAppsEntry {
    if (Test-RegistryKey -SubKey $RegisteredAppsSubKey) {
        if (Remove-RegistryValue -SubKey $RegisteredAppsSubKey -Name $RegisteredAppName) { $script:removed++ }
    }
    if (Test-RegistryKey -SubKey $CapabilitiesSubKey) { Remove-RegistryTree -SubKey $CapabilitiesSubKey }
    Remove-RegistryKeyIfEmpty -SubKey $CapabilitiesParentSubKey
}

function Read-OwnedSet {
    $owned = New-Object 'System.Collections.Generic.HashSet[string]'
    $raw = Read-RegistryValue -SubKey $ProgIdSubKey -Name $OwnedValue
    if ($raw) {
        foreach ($token in ($raw -split ';')) { if ($token) { [void]$owned.Add($token) } }
    }
    return , $owned
}

# --- Register --------------------------------------------------------------
function Register-MinkFileType {
    param([string]$ResolvedIcon, [string]$ResolvedEditor, [string]$ResolvedMinkExe, [string]$AppIcon)

    $currentOwner = Read-RegistryValue -SubKey $ExtensionSubKey
    if ($currentOwner -and $currentOwner -ne $ProgId -and -not $Force) {
        Stop-WithError ("The extension $Extension is currently associated with '$currentOwner'. " +
            "Nothing was changed. Re-run with -Force to replace it, or remove that association first.")
    }

    $owned = Read-OwnedSet

    Set-RegistryValue -SubKey $ExtensionSubKey -Name '' -Value $ProgId
    Set-RegistryValue -SubKey $ExtensionSubKey -Name 'Content Type' -Value $ContentType
    Set-RegistryValue -SubKey $ExtensionSubKey -Name 'PerceivedType' -Value $PerceivedType

    Set-RegistryValue -SubKey $ProgIdSubKey -Name '' -Value $TypeName
    Set-RegistryValue -SubKey $ProgIdSubKey -Name 'FriendlyTypeName' -Value $TypeName

    # Quoted because installation paths contain spaces; ",0" selects the first image.
    Set-RegistryValue -SubKey $DefaultIconSubKey -Name '' -Value ('"{0}",0' -f $ResolvedIcon)

    # Double-clicking runs the default verb, and a default verb must never run or build
    # the file. With an editor bound, "open" is the default and is labelled "Edit". With no
    # editor, the "edit" verb (Notepad) is the default. Without any verb, Windows asks.
    $defaultVerb = $null
    if ($ResolvedEditor) {
        Set-RegistryValue -SubKey $OpenVerbSubKey -Name '' -Value $VerbLabels['edit']
        Set-RegistryValue -SubKey $OpenCommandSubKey -Name '' -Value ('"{0}" "%1"' -f $ResolvedEditor)
        $defaultVerb = 'open'
    }
    elseif (Test-RegistryKey -SubKey $OpenVerbSubKey) {
        # A previous run bound an editor; with no -Editor the association must go back to
        # letting Windows choose, so the MINK-owned open verb is removed.
        Remove-RegistryTree -SubKey $OpenVerbSubKey
        $notes.Add('removed the previously bound editor command (-Editor not given)')
    }

    $wanted = @()
    if (-not $NoContextMenu) {
        $wanted = @($Verbs)
        # The edit verb is the safe default for double-click, so it is always there when
        # any entry is; with an editor bound the "open" verb already does that job.
        if ($ResolvedEditor) { $wanted = @($wanted | Where-Object { $_ -ne 'edit' }) }
        elseif ($wanted -notcontains 'edit') { $wanted += 'edit' }
        if (-not $ResolvedEditor -and $wanted -contains 'edit') { $defaultVerb = 'edit' }
    }
    Register-ContextMenu -ResolvedMinkExe $ResolvedMinkExe -ResolvedEditor $ResolvedEditor `
        -AppIcon $AppIcon -Wanted $wanted
    if ($defaultVerb) {
        Set-RegistryValue -SubKey $ShellSubKey -Name '' -Value $defaultVerb
    }
    elseif (Test-RegistryKey -SubKey $ShellSubKey) {
        [void](Remove-RegistryValue -SubKey $ShellSubKey -Name '')
    }
    Remove-RegistryKeyIfEmpty -SubKey $ShellSubKey

    # Make the MINK file type appear in Explorer's "Open with" list. The value is
    # REG_NONE with no data, which is what the shell expects for a ProgID list.
    $openWith = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($OpenWithSubKey)
    if (-not $openWith) { Stop-WithError "Could not open HKCU:\$OpenWithSubKey for writing." }
    try {
        $openWith.SetValue($ProgId, [byte[]]@(), [Microsoft.Win32.RegistryValueKind]::None)
    }
    finally { $openWith.Dispose() }

    Register-Facilities -Owned $owned

    # What this run added beyond the basics, so -Uninstall removes exactly that.
    Set-RegistryValue -SubKey $ProgIdSubKey -Name $OwnedValue -Value (($owned | Sort-Object) -join ';')

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
    $owned = Read-OwnedSet

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
        if (Test-Owned $owned 'NewMenu') { Remove-RegistryTree -SubKey $NewMenuSubKey }
        Remove-OwnedPreview -Owned $owned
    }

    if (Test-RegistryKey -SubKey $ProgIdSubKey) {
        Remove-RegistryTree -SubKey $ProgIdSubKey
        $notes.Add("removed HKCU:\$ProgIdSubKey")
    }

    Remove-DefaultAppsEntry

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
function Write-Info {
    param([string]$Text = '')
    if (-not $Quiet) { Write-Host $Text }
}

Write-Info 'MINK file association (.mink)'
Write-Info ''

if ($Uninstall) {
    Unregister-MinkFileType
    Update-ShellAssociations
    Write-Info 'State'
    Write-PathState -Label 'extension' -Value "$Extension is no longer owned by $ProgId"
    Write-PathState -Label 'prog id' -Value "HKCU:\$ProgIdSubKey removed"
    Write-PathState -Label 'removed' -Value "$removed MINK-owned value(s)"
    Write-PathState -Label 'explorer' -Value 'association change broadcast, icon cache refreshed'
    Write-Info ''
    foreach ($note in $notes) { Write-Info "  note        : $note" }
    Write-Info ''
    Write-Info 'Done. MINK-owned file-association entries were removed.'
    exit 0
}

$resolvedInstallDir = Resolve-InstallDirectory -ExplicitDir $InstallDir -ExplicitExe $MinkExe
$resolvedIcon = Resolve-FileIcon -Explicit $IconPath -ResolvedInstallDir $resolvedInstallDir
$resolvedMinkExe = $null
if ($resolvedInstallDir) { $resolvedMinkExe = Join-Path $resolvedInstallDir 'mink.exe' }
$appIcon = Resolve-AppIcon -ResolvedInstallDir $resolvedInstallDir
$resolvedEditor = $null
if ($Editor) {
    $resolvedEditor = Resolve-EditorPath -Value $Editor
    if ([System.IO.Path]::GetExtension($resolvedEditor) -ne '.exe') {
        Write-Warning ("The editor '$resolvedEditor' is not an .exe; Windows opens files with it " +
            "through a script shim. Pass the real executable when one exists.")
    }
}

Register-MinkFileType -ResolvedIcon $resolvedIcon -ResolvedEditor $resolvedEditor `
    -ResolvedMinkExe $resolvedMinkExe -AppIcon $appIcon
Update-ShellAssociations

Write-Info 'State'
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
$shownVerbs = @()
if (-not $NoContextMenu) {
    $shownVerbs = @($Verbs)
    if ($resolvedEditor) { $shownVerbs = @($shownVerbs | Where-Object { $_ -ne 'edit' }) + 'edit' }
    elseif ($shownVerbs -notcontains 'edit') { $shownVerbs += 'edit' }
}
if ($shownVerbs.Count -gt 0) {
    Write-PathState -Label 'right-click' -Value (($shownVerbs | ForEach-Object { $VerbLabels[$_] }) -join ', ')
}
else {
    Write-PathState -Label 'right-click' -Value 'no MINK entries'
}
Write-PathState -Label 'new menu' -Value $(if ($NoNewMenu) { 'off' } else { 'MINK Source File' })
Write-PathState -Label 'preview' -Value $(if ($NoPreview) { 'off' } else { 'plain text preview and search' })
Write-PathState -Label 'default apps' -Value $(if ($NoDefaultApps) { 'off' } else { 'MINK is listed' })
Write-PathState -Label 'registry' -Value "HKCU:\$ExtensionSubKey -> $ProgId (per-user)"
Write-PathState -Label 'explorer' -Value 'association change broadcast, icon cache refreshed'
Write-Info ''
foreach ($note in $notes) { Write-Info "  note        : $note" }
Write-Info ''
if ($resolvedEditor) {
    Write-Info 'Done. Double-clicking a .mink file opens it in the registered editor.'
}
else {
    Write-Info 'Done. Double-clicking a .mink file asks Windows which application to use the first'
    Write-Info 'time and remembers that choice; the source is never compiled or run.'
}
exit 0
