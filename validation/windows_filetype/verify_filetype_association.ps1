#Requires -Version 5.1
<#
.SYNOPSIS
    Clean-room validation of the `.mink` Windows file association.

.DESCRIPTION
    Builds a temporary "installed package" tree, registers the file type with
    npm\mink\windows\register-mink-filetype.ps1, and checks what Windows itself reports
    rather than what the registry files claim. Every association entry this test writes
    is restored to its previous state in a finally block, so running it on a developer
    machine does not leave the developer's own `.mink` association changed.

    Steps:
      1. snapshot the HKCU keys this test touches (recursively)
      2. build a temporary install tree whose path contains a space
      3. take-over safety: a `.mink` association owned by another application is not
         replaced without -Force, and -Force preserves that application's own entries
      4. uninstall after a takeover restores the previous owner
      5. from a clean extension, register through the package layout alone and check the
         file type, source-file icon, right-click entries, New menu, preview, Default apps
         entry and open behaviour as the shell reports them
     5b. (step 6b below) each option turns its facility off, and a registered verb runs
      6. repeated registration is idempotent (same values, same subkeys, one session)
      7. with an editor bound, opening the file through the association reaches the
         editor and never compiles or runs the source
      8. re-registering without -Editor removes the stale open command
      9. the icon path round-trips from a Unicode installation directory
     10. uninstall from a clean extension removes every MINK-owned entry
     11. the real npm package layout (icon only in windows\, not bin\) resolves the icon
     12. a source checkout with no installation present resolves the icon beside the script
     13. a missing, malformed or non-existent icon refuses and changes nothing
     14. the shipped kit is complete, portable and registers exactly one extension
     15. the machine is restored to its starting state

    Run with Windows PowerShell 5.1:

        powershell -ExecutionPolicy Bypass -File validation\windows_filetype\verify_filetype_association.ps1

    Exit code 0 when every check passes, 1 otherwise.
#>
[CmdletBinding()]
param(
    [string]$KitDir,
    [switch]$KeepWorkDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Extension = '.mink'
$ProgId = 'MINK.SourceFile'
$TypeName = 'MINK Source File'
$ContentType = 'text/x-mink'

$ExtensionSubKey = 'Software\Classes\.mink'
$ProgIdSubKey = 'Software\Classes\MINK.SourceFile'
$DefaultIconSubKey = 'Software\Classes\MINK.SourceFile\DefaultIcon'
$OpenCommandSubKey = 'Software\Classes\MINK.SourceFile\shell\open\command'
$FileExtsSubKey = 'Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.mink'
$OpenWithSubKey = 'Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.mink\OpenWithProgids'
$ShellSubKey = 'Software\Classes\MINK.SourceFile\shell'
$NewMenuSubKey = 'Software\Classes\.mink\ShellNew'
$PersistentHandlerSubKey = 'Software\Classes\.mink\PersistentHandler'
$PreviewSubKey = 'Software\Classes\.mink\shellex\{8895b1c6-b41f-4c1c-a562-0d564250836f}'
$PlainTextFilter = '{5e941d80-bf96-11cd-b579-08002b30bfeb}'
$CapabilitiesSubKey = 'Software\MINK\Capabilities'
$MinkAppSubKey = 'Software\MINK'
$RegisteredAppsSubKey = 'Software\RegisteredApplications'
$UninstallSubKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\MINK'

if (-not $KitDir) {
    $KitDir = Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'npm\mink\windows'
}
$KitDir = (Resolve-Path -LiteralPath $KitDir).Path
$registerScript = Join-Path $KitDir 'register-mink-filetype.ps1'
if (-not (Test-Path -LiteralPath $registerScript)) {
    Write-Host "ERROR: registration script not found: $registerScript" -ForegroundColor Red
    exit 1
}

# --- tiny assertion framework ----------------------------------------------
# Script-scoped so the counters survive the function boundaries below.
$script:passed = 0
$script:failed = 0

function Write-Step {
    param([string]$Title)
    Write-Host ''
    Write-Host "[$Title]"
}

function Assert-True {
    param([string]$What, [bool]$Condition, [string]$Detail = '')
    if ($Condition) {
        $script:passed++
        Write-Host ("  PASS  {0}" -f $What) -ForegroundColor Green
    }
    else {
        $script:failed++
        $suffix = if ($Detail) { " ($Detail)" } else { '' }
        Write-Host ("  FAIL  {0}{1}" -f $What, $suffix) -ForegroundColor Red
    }
}

function Assert-Equal {
    param([string]$What, $Expected, $Actual)
    $expectedText = if ($null -eq $Expected) { '<none>' } else { "$Expected" }
    $actualText = if ($null -eq $Actual) { '<none>' } else { "$Actual" }
    Assert-True -What $What -Condition ($expectedText -eq $actualText) `
        -Detail "expected '$expectedText', got '$actualText'"
}

function Write-Note {
    param([string]$Message)
    Write-Host ("  note  {0}" -f $Message) -ForegroundColor DarkGray
}

# --- registry helpers ------------------------------------------------------
function Get-CurrentUserKey {
    param([string]$SubKey, [switch]$Writable)
    return [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($SubKey, [bool]$Writable)
}

function Get-KeyValue {
    param([string]$SubKey, [string]$Name = '')
    $key = Get-CurrentUserKey -SubKey $SubKey
    if (-not $key) { return $null }
    try { return $key.GetValue($Name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
    finally { $key.Dispose() }
}

function Get-KeyValueNames {
    param([string]$SubKey)
    $key = Get-CurrentUserKey -SubKey $SubKey
    if (-not $key) { return @() }
    try { return @($key.GetValueNames()) }
    finally { $key.Dispose() }
}

function Get-KeySubNames {
    param([string]$SubKey)
    $key = Get-CurrentUserKey -SubKey $SubKey
    if (-not $key) { return @() }
    try { return @($key.GetSubKeyNames()) }
    finally { $key.Dispose() }
}

function Test-KeyExists {
    param([string]$SubKey)
    return ($null -ne (Get-CurrentUserKey -SubKey $SubKey))
}

function Remove-KeyIfPresent {
    param([string]$SubKey)
    [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($SubKey, $false)
}

function Set-KeyValue {
    param([string]$SubKey, [string]$Name, [string]$Value)
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($SubKey)
    try { $key.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]::String) }
    finally { $key.Dispose() }
}

# A flat "name=value" signature of a key's own values, used to prove that a refused
# registration wrote nothing at all.
function Get-RegistrySignature {
    param([string]$SubKey)
    $key = Get-CurrentUserKey -SubKey $SubKey
    if (-not $key) { return '<absent>' }
    try {
        $entries = foreach ($name in $key.GetValueNames()) {
            "{0}={1}" -f $name, $key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        }
        return (($entries | Sort-Object) -join ';')
    }
    finally { $key.Dispose() }
}

# ICONDIR header: reserved 0, type 1 (icon), then the frame count.
function Get-IconFrameCount {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return -1 }
    try { $bytes = [System.IO.File]::ReadAllBytes($Path) }
    catch { return -1 }
    if ($bytes.Length -lt 6) { return -1 }
    if ($bytes[0] -ne 0 -or $bytes[1] -ne 0 -or $bytes[2] -ne 1 -or $bytes[3] -ne 0) { return -1 }
    return ([int]$bytes[4] + 256 * [int]$bytes[5])
}

# Snapshots are recursive (values and subkeys), so restoring a key puts the machine back
# exactly as it was even if the developer already had their own .mink association.
function Get-Snapshot {
    param([string]$SubKey)
    $key = Get-CurrentUserKey -SubKey $SubKey
    if (-not $key) { return $null }
    try {
        $entries = New-Object System.Collections.Generic.List[object]
        foreach ($name in $key.GetValueNames()) {
            $entries.Add([pscustomobject]@{
                    Name  = $name
                    Value = $key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                    Kind  = $key.GetValueKind($name)
                })
        }
        $children = @{}
        foreach ($name in $key.GetSubKeyNames()) {
            $children[$name] = Get-Snapshot -SubKey (Join-Path $SubKey $name)
        }
        return [pscustomobject]@{ Values = $entries; Children = $children }
    }
    finally { $key.Dispose() }
}

function Restore-Snapshot {
    param([string]$SubKey, $Snapshot)
    $currentUser = [Microsoft.Win32.Registry]::CurrentUser
    $currentUser.DeleteSubKeyTree($SubKey, $false)
    if ($null -eq $Snapshot) { return }
    $key = $currentUser.CreateSubKey($SubKey)
    try {
        foreach ($entry in $Snapshot.Values) {
            $key.SetValue($entry.Name, $entry.Value, $entry.Kind)
        }
    }
    finally { $key.Dispose() }
    foreach ($name in $Snapshot.Children.Keys) {
        Restore-Snapshot -SubKey (Join-Path $SubKey $name) -Snapshot $Snapshot.Children[$name]
    }
}

function Test-Snapshot {
    param([string]$SubKey, $Snapshot)
    $key = Get-CurrentUserKey -SubKey $SubKey
    if (-not $key) { return ($null -eq $Snapshot) }
    try {
        if ($null -eq $Snapshot) { return $false }
        foreach ($name in $key.GetValueNames()) {
            $expected = $Snapshot.Values | Where-Object { $_.Name -eq $name }
            if (-not $expected) { return $false }
            $current = $key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            if ("$current" -ne "$($expected.Value)") { return $false }
        }
        foreach ($name in $Snapshot.Values | ForEach-Object { $_.Name }) {
            if (-not ($key.GetValueNames() -contains $name)) { return $false }
        }
        if (@($key.GetSubKeyNames()).Count -ne @($Snapshot.Children.Keys).Count) { return $false }
        foreach ($name in $key.GetSubKeyNames()) {
            if (-not $Snapshot.Children.ContainsKey($name)) { return $false }
            if (-not (Test-Snapshot -SubKey (Join-Path $SubKey $name) -Snapshot $Snapshot.Children[$name])) {
                return $false
            }
        }
        return $true
    }
    finally { $key.Dispose() }
}

# --- shell helpers ---------------------------------------------------------
if (-not ('MinkFiletypeTest.ShellApi' -as [type])) {
    Add-Type -Namespace MinkFiletypeTest -Name ShellApi -MemberDefinition @'
[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct SHFILEINFO {
    public System.IntPtr hIcon;
    public int iIcon;
    public uint dwAttributes;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] public string szDisplayName;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 80)] public string szTypeName;
}

[DllImport("shell32.dll", CharSet = CharSet.Unicode)]
public static extern System.IntPtr SHGetFileInfo(string pszPath, uint dwFileAttributes,
    ref SHFILEINFO psfi, uint cbFileInfo, uint uFlags);

[DllImport("user32.dll")]
public static extern bool DestroyIcon(System.IntPtr hIcon);

[DllImport("shlwapi.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
public static extern void AssocQueryString(int flags, int str, string assoc, string extra,
    System.Text.StringBuilder outBuffer, ref uint outBufferSize);
'@
}

$SHGFI_ICON = 0x000000100
$SHGFI_TYPENAME = 0x000000400
$ASSOCF_NONE = 0
$ASSOCSTR_COMMAND = 1
$ASSOCSTR_FRIENDLYDOCNAME = 3
$ASSOCSTR_DEFAULTICON = 15
$ASSOCSTR_PROGID = 20

function Get-ShellFileInfo {
    param([string]$Path, [uint32]$Flags)
    $info = New-Object MinkFiletypeTest.ShellApi+SHFILEINFO
    $size = [System.Runtime.InteropServices.Marshal]::SizeOf($info)
    [void][MinkFiletypeTest.ShellApi]::SHGetFileInfo($Path, 0, [ref]$info, $size, $Flags)
    return $info
}

function Get-AssocString {
    param([string]$Association, [int]$StringType, [string]$Extra = $null)
    $bufferSize = [uint32]1024
    $buffer = New-Object System.Text.StringBuilder 1024
    try {
        [MinkFiletypeTest.ShellApi]::AssocQueryString($ASSOCF_NONE, $StringType, $Association,
            $Extra, $buffer, [ref]$bufferSize)
        return $buffer.ToString()
    }
    catch {
        return $null
    }
}

# --- invocation helpers ----------------------------------------------------
function Invoke-RegisterScript {
    param([string[]]$Arguments, [string]$ScriptPath)
    if (-not $ScriptPath) { $ScriptPath = $script:registerScript }
    $output = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $ScriptPath @Arguments 2>&1
    return [pscustomobject]@{
        ExitCode = $LASTEXITCODE
        Output   = (@($output) -join "`n")
    }
}

function Show-Output {
    param([string]$Output)
    foreach ($line in ($Output -split "`n")) {
        if ($line.Trim()) { Write-Note $line.TrimEnd() }
    }
}

$work = Join-Path $env:TEMP ("mink-filetype-" + [guid]::NewGuid().ToString('N'))
$package = Join-Path $work 'package with space'
$packageBin = Join-Path $package 'bin'
$probeLog = Join-Path $work 'probe.log'
$snapshots = @{}
$result = 0

function Restore-MachineState {
    Restore-Snapshot -SubKey $ExtensionSubKey $snapshots[0]
    Restore-Snapshot -SubKey $ProgIdSubKey $snapshots[1]
    Restore-Snapshot -SubKey $FileExtsSubKey $snapshots[2]
    Restore-Snapshot -SubKey $MinkAppSubKey $snapshots[3]
    Restore-Snapshot -SubKey $UninstallSubKey $snapshots[5]
    # RegisteredApplications is shared with every other program, so only the MINK value
    # is put back; the other values are never read or rewritten.
    if ($null -eq $snapshots[4]) {
        $key = Get-CurrentUserKey -SubKey $RegisteredAppsSubKey -Writable
        if ($key) {
            try { $key.DeleteValue('MINK', $false) } finally { $key.Dispose() }
        }
    }
    else {
        Set-KeyValue -SubKey $RegisteredAppsSubKey -Name 'MINK' -Value $snapshots[4]
    }
}

try {
    Write-Host 'MINK .mink file association - clean-room validation'
    Write-Host '  registry : HKCU (per-user)'
    Write-Host "  work dir : $work"

    # ---------------------------------------------------------------- step 1
    Write-Step '1. snapshot'
    $snapshots[0] = Get-Snapshot -SubKey $ExtensionSubKey
    $snapshots[1] = Get-Snapshot -SubKey $ProgIdSubKey
    $snapshots[2] = Get-Snapshot -SubKey $FileExtsSubKey
    $snapshots[3] = Get-Snapshot -SubKey $MinkAppSubKey
    $snapshots[4] = Get-KeyValue -SubKey $RegisteredAppsSubKey -Name 'MINK'
    $snapshots[5] = Get-Snapshot -SubKey $UninstallSubKey
    for ($index = 0; $index -lt 4; $index++) {
        $state = if ($null -eq $snapshots[$index]) { 'absent' } else { "$($snapshots[$index].Values.Count) value(s)" }
        Write-Note "snapshot ${index}: $state"
    }
    Assert-True -What 'previous association state captured' -Condition $true

    # ---------------------------------------------------------------- step 2
    Write-Step '2. temporary install tree (path contains a space)'
    New-Item -ItemType Directory -Path (Join-Path $package 'windows') -Force | Out-Null
    New-Item -ItemType Directory -Path $packageBin -Force | Out-Null
    Copy-Item -Path (Join-Path $KitDir '*') -Destination (Join-Path $package 'windows') -Force
    Copy-Item -Path (Join-Path $KitDir '*.ico') -Destination $packageBin
    # A stand-in for mink.exe: registration only notices that an installation exists, it
    # never runs the compiler.
    Set-Content -Path (Join-Path $packageBin 'mink.exe') -Value 'stand-in' -Encoding Ascii
    $installIcon = (Resolve-Path -LiteralPath (Join-Path $packageBin 'mink-file.ico')).Path
    $helloMink = Join-Path $package 'hello.mink'
    Set-Content -Path $helloMink -Value 'fn main() -> Int { return 0; }' -Encoding Ascii
    Assert-True -What 'temporary package tree created' -Condition (Test-Path -LiteralPath $helloMink)

    # ---------------------------------------------------------------- step 3
    Write-Step '3. an association owned by another application is not taken over'
    Remove-KeyIfPresent -SubKey $ExtensionSubKey
    Remove-KeyIfPresent -SubKey $ProgIdSubKey
    Set-KeyValue -SubKey $ExtensionSubKey -Name '' -Value 'Contoso.SourceFile'
    Set-KeyValue -SubKey $ExtensionSubKey -Name 'DecoyOwned' -Value 'keep'
    Set-KeyValue -SubKey "$ExtensionSubKey\OpenWithProgids" -Name 'Contoso.SourceFile' -Value ''
    Set-KeyValue -SubKey $NewMenuSubKey -Name 'FileName' -Value 'C:\Contoso\template.txt'

    $run = Invoke-RegisterScript -Arguments @('-InstallDir', $package)
    Assert-Equal -What 'refuses without -Force' -Expected 1 -Actual $run.ExitCode
    Assert-True -What 'refusal names the current owner' -Condition ($run.Output -match 'Contoso\.SourceFile')
    Assert-Equal -What 'refusal changes nothing' -Expected 'Contoso.SourceFile' `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey)
    Assert-True -What 'no MINK ProgID was created' -Condition (-not (Test-KeyExists -SubKey $ProgIdSubKey))

    $run = Invoke-RegisterScript -Arguments @('-InstallDir', $package, '-Force')
    Assert-Equal -What 'registration with -Force exits 0' -Expected 0 -Actual $run.ExitCode
    if ($run.ExitCode -ne 0) { Show-Output -Output $run.Output }
    Assert-Equal -What 'extension now points at the MINK ProgID' -Expected $ProgId `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey)
    Assert-Equal -What "the other application's value survives" -Expected 'keep' `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey -Name 'DecoyOwned')
    Assert-True -What "the other application's subkey survives" `
        -Condition ((Get-KeySubNames -SubKey $ExtensionSubKey) -contains 'OpenWithProgids')
    Assert-Equal -What 'previous owner recorded for undo' -Expected 'Contoso.SourceFile' `
        -Actual (Get-KeyValue -SubKey $ProgIdSubKey -Name 'PreviousDotMinkOwner')
    Assert-Equal -What "the other application's New-menu entry is not replaced" `
        -Expected 'C:\Contoso\template.txt' -Actual (Get-KeyValue -SubKey $NewMenuSubKey -Name 'FileName')
    Assert-True -What 'the New-menu entry is not recorded as MINK-owned' `
        -Condition ((Get-KeyValue -SubKey $ProgIdSubKey -Name 'MinkOwned') -notmatch 'NewMenu')
    Write-Note ('while another application''s class-level OpenWithProgids list exists, Windows treats')
    Write-Note ('.mink as "open with" and asks the user to confirm the application once.')

    # ---------------------------------------------------------------- step 4
    Write-Step '4. uninstall after a takeover'
    $run = Invoke-RegisterScript -Arguments @('-Uninstall')
    Assert-Equal -What 'uninstall exits 0' -Expected 0 -Actual $run.ExitCode
    Assert-True -What 'MINK ProgID removed' -Condition (-not (Test-KeyExists -SubKey $ProgIdSubKey))
    Assert-Equal -What 'previous owner restored' -Expected 'Contoso.SourceFile' `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey)
    Assert-Equal -What 'MINK content type removed' -Expected $null `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey -Name 'Content Type')
    Assert-Equal -What "the other application's value still present" -Expected 'keep' `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey -Name 'DecoyOwned')
    Assert-True -What "the other application's ProgID still present" `
        -Condition ((Get-KeyValueNames -SubKey "$ExtensionSubKey\OpenWithProgids") -contains 'Contoso.SourceFile')
    Assert-True -What 'extension key kept for the other application' `
        -Condition (Test-KeyExists -SubKey $ExtensionSubKey)
    Assert-Equal -What "the other application's New-menu entry survives uninstall" `
        -Expected 'C:\Contoso\template.txt' -Actual (Get-KeyValue -SubKey $NewMenuSubKey -Name 'FileName')

    # ---------------------------------------------------------------- step 5
    Write-Step '5. clean extension: file type, icon and open behaviour'
    Remove-KeyIfPresent -SubKey $ExtensionSubKey
    Remove-KeyIfPresent -SubKey $ProgIdSubKey
    # The copy inside the temporary package, run with no arguments: it must find
    # bin\mink.exe and bin\mink-file.ico relative to itself.
    $packageScript = Join-Path $package 'windows\register-mink-filetype.ps1'
    $run = Invoke-RegisterScript -Arguments @() -ScriptPath $packageScript
    Assert-Equal -What 'registration through the package layout alone exits 0' -Expected 0 -Actual $run.ExitCode
    if ($run.ExitCode -ne 0) { Show-Output -Output $run.Output }

    Assert-Equal -What 'extension points at the MINK ProgID' -Expected $ProgId `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey)
    Assert-Equal -What 'content type' -Expected $ContentType `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey -Name 'Content Type')
    Assert-Equal -What 'perceived type' -Expected 'text' `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey -Name 'PerceivedType')
    Assert-Equal -What 'display name' -Expected $TypeName `
        -Actual (Get-KeyValue -SubKey $ProgIdSubKey)
    Assert-Equal -What 'default icon is the designed .mink icon, quoted, index 0' `
        -Expected ('"{0}",0' -f $installIcon) -Actual (Get-KeyValue -SubKey $DefaultIconSubKey)
    Assert-True -What 'the registered icon file exists' -Condition (Test-Path -LiteralPath $installIcon)
    Assert-Equal -What 'the registered icon is a valid 7-frame ICO' `
        -Expected 7 -Actual (Get-IconFrameCount -Path $installIcon)
    Assert-True -What 'no open command: a .mink file is never executed' `
        -Condition (-not (Test-KeyExists -SubKey $OpenCommandSubKey))

    # Facilities beyond the icon.
    $installExe = (Resolve-Path -LiteralPath (Join-Path $packageBin 'mink.exe')).Path
    $installAppIcon = (Resolve-Path -LiteralPath (Join-Path $packageBin 'mink-app.ico')).Path
    Assert-Equal -What 'double-click runs the safe default verb, edit' -Expected 'edit' `
        -Actual (Get-KeyValue -SubKey $ShellSubKey)
    foreach ($pair in @(@('run', 'Run with MINK'), @('check', 'Check with MINK'),
            @('build', 'Build executable'), @('edit', 'Edit'))) {
        Assert-Equal -What ("right-click entry label: " + $pair[0]) -Expected $pair[1] `
            -Actual (Get-KeyValue -SubKey ("$ShellSubKey\" + $pair[0]))
    }
    foreach ($verb in @('run', 'check', 'build')) {
        Assert-Equal -What "the $verb entry runs mink.exe shell $verb on the file" `
            -Expected ('"{0}" shell {1} "%1"' -f $installExe, $verb) `
            -Actual (Get-KeyValue -SubKey "$ShellSubKey\$verb\command")
        Assert-Equal -What "the $verb entry carries the MINK icon" `
            -Expected ('"{0}",0' -f $installAppIcon) -Actual (Get-KeyValue -SubKey "$ShellSubKey\$verb" -Name 'Icon')
    }
    Assert-Equal -What 'run is single-selection, so many files cannot start many programs' -Expected 'Single' `
        -Actual (Get-KeyValue -SubKey "$ShellSubKey\run" -Name 'MultiSelectModel')
    Assert-Equal -What 'the edit entry opens Notepad when no editor is bound' `
        -Expected ('"{0}" "%1"' -f (Join-Path $env:SystemRoot 'notepad.exe')) `
        -Actual (Get-KeyValue -SubKey "$ShellSubKey\edit\command")
    $templatePath = (Resolve-Path -LiteralPath (Join-Path $package 'windows\template.mink')).Path
    Assert-Equal -What 'New menu copies the shipped template' -Expected $templatePath `
        -Actual (Get-KeyValue -SubKey $NewMenuSubKey -Name 'FileName')
    Assert-Equal -What 'Windows Search reads .mink as plain text' -Expected $PlainTextFilter `
        -Actual (Get-KeyValue -SubKey $PersistentHandlerSubKey)
    $txtPreview = $null
    $classesKey = [Microsoft.Win32.Registry]::ClassesRoot.OpenSubKey('.txt\shellex\{8895b1c6-b41f-4c1c-a562-0d564250836f}')
    if ($classesKey) { try { $txtPreview = $classesKey.GetValue('') } finally { $classesKey.Dispose() } }
    if ($txtPreview) {
        Assert-Equal -What 'the preview pane uses the same handler as .txt' -Expected $txtPreview `
            -Actual (Get-KeyValue -SubKey $PreviewSubKey)
    }
    else { Write-Note 'this Windows has no .txt preview handler; preview registration is skipped by design' }
    Assert-Equal -What 'MINK is listed under Default apps' -Expected 'MINK' `
        -Actual (Get-KeyValue -SubKey $CapabilitiesSubKey -Name 'ApplicationName')
    Assert-Equal -What 'the Default apps entry maps .mink to the MINK file type' -Expected $ProgId `
        -Actual (Get-KeyValue -SubKey "$CapabilitiesSubKey\FileAssociations" -Name '.mink')
    Assert-Equal -What 'MINK is registered as an application' -Expected 'Software\MINK\Capabilities' `
        -Actual (Get-KeyValue -SubKey $RegisteredAppsSubKey -Name 'MINK')
    Assert-True -What 'every facility is recorded as MINK-owned' `
        -Condition ((Get-KeyValue -SubKey $ProgIdSubKey -Name 'MinkOwned') -match 'NewMenu.*PersistentHandler')
    $shellRun = Get-AssocString -Association $Extension -StringType $ASSOCSTR_COMMAND -Extra 'run'
    Assert-Equal -What 'the shell resolves the run verb to mink.exe shell run' `
        -Expected ('"{0}" shell run "{1}"' -f $installExe, '%1') -Actual $shellRun
    Assert-True -What 'MINK is offered in "Open with"' `
        -Condition ((Get-KeyValueNames -SubKey $OpenWithSubKey) -contains $ProgId)

    $typeInfo = Get-ShellFileInfo -Path $helloMink -Flags $SHGFI_TYPENAME
    Assert-Equal -What 'Explorer type name' -Expected $TypeName -Actual $typeInfo.szTypeName
    $friendly = Get-AssocString -Association $Extension -StringType $ASSOCSTR_FRIENDLYDOCNAME
    Assert-Equal -What 'association friendly name' -Expected $TypeName -Actual $friendly
    $iconViaShell = Get-AssocString -Association $Extension -StringType $ASSOCSTR_DEFAULTICON
    Assert-Equal -What 'the shell icon comes from mink-file.ico' -Expected ('"{0}",0' -f $installIcon) `
        -Actual $iconViaShell
    $progIdViaShell = Get-AssocString -Association $Extension -StringType $ASSOCSTR_PROGID
    Assert-Equal -What 'association ProgID' -Expected $ProgId -Actual $progIdViaShell
    $iconInfo = Get-ShellFileInfo -Path $helloMink -Flags $SHGFI_ICON
    Assert-True -What 'the shell resolves an icon for a .mink file' -Condition ($iconInfo.hIcon -ne [IntPtr]::Zero)
    if ($iconInfo.hIcon -ne [IntPtr]::Zero) {
        [void][MinkFiletypeTest.ShellApi]::DestroyIcon($iconInfo.hIcon)
    }
    $command = Get-AssocString -Association $Extension -StringType $ASSOCSTR_COMMAND
    Assert-True -What 'the resolved open command never runs mink.exe' `
        -Condition (-not $command -or $command -notmatch 'mink\.exe')
    Write-Note ("open command resolved by the shell: " + $(if ($command) { $command } else { '<none>' }))
    Write-Note ("file name as reported for the fixture: " + (Get-Item -LiteralPath $helloMink).Name)
    Write-Note 'cmd.exe assoc/ftype read machine-wide (HKLM) classes only, so they do not report a'
    Write-Note 'per-user registration; the shell APIs above are what Explorer actually uses.'

    # ---------------------------------------------------------------- step 6
    Write-Step '6. repeated registration is idempotent'
    $before = @{
        extension  = (Get-KeyValueNames -SubKey $ExtensionSubKey) -join ','
        progId     = (Get-KeyValueNames -SubKey $ProgIdSubKey) -join ','
        subkeys    = (Get-KeySubNames -SubKey $ProgIdSubKey) -join ','
        icon       = Get-KeyValue -SubKey $DefaultIconSubKey
        typeName   = (Get-ShellFileInfo -Path $helloMink -Flags $SHGFI_TYPENAME).szTypeName
    }
    $run = Invoke-RegisterScript -Arguments @() -ScriptPath $packageScript
    Assert-Equal -What 'second registration exits 0' -Expected 0 -Actual $run.ExitCode
    Assert-Equal -What 'no duplicated extension values' -Expected $before.extension `
        -Actual ((Get-KeyValueNames -SubKey $ExtensionSubKey) -join ',')
    Assert-Equal -What 'no duplicated ProgID values' -Expected $before.progId `
        -Actual ((Get-KeyValueNames -SubKey $ProgIdSubKey) -join ',')
    Assert-Equal -What 'no leftover ProgID subkeys' -Expected $before.subkeys `
        -Actual ((Get-KeySubNames -SubKey $ProgIdSubKey) -join ',')
    Assert-Equal -What 'icon unchanged' -Expected $before.icon -Actual (Get-KeyValue -SubKey $DefaultIconSubKey)
    Assert-Equal -What 'Explorer type name unchanged' -Expected $before.typeName `
        -Actual (Get-ShellFileInfo -Path $helloMink -Flags $SHGFI_TYPENAME).szTypeName
    # Two runs in one PowerShell session exercise the Add-Type guard in the script.
    $first = & $packageScript 2>&1 | Out-String
    $second = & $packageScript 2>&1 | Out-String
    Assert-True -What 'repeated runs in one session do not fail on the shell-refresh type' `
        -Condition ($first -notmatch 'already exists' -and $second -notmatch 'already exists')

    # ---------------------------------------------------------------- step 6b
    Write-Step '6b. options turn facilities off, and a right-click verb really runs'
    $run = Invoke-RegisterScript -Arguments @('-NoContextMenu', '-NoNewMenu', '-NoPreview', '-NoDefaultApps') `
        -ScriptPath $packageScript
    Assert-Equal -What 'registration with every option off exits 0' -Expected 0 -Actual $run.ExitCode
    Assert-True -What 'no right-click entries' -Condition (-not (Test-KeyExists -SubKey $ShellSubKey))
    Assert-True -What 'no New-menu entry' -Condition (-not (Test-KeyExists -SubKey $NewMenuSubKey))
    Assert-True -What 'no search filter' -Condition (-not (Test-KeyExists -SubKey $PersistentHandlerSubKey))
    Assert-True -What 'no preview handler' -Condition (-not (Test-KeyExists -SubKey $PreviewSubKey))
    Assert-True -What 'not listed under Default apps' -Condition (-not (Test-KeyExists -SubKey $CapabilitiesSubKey))
    Assert-Equal -What 'no RegisteredApplications value' -Expected $null `
        -Actual (Get-KeyValue -SubKey $RegisteredAppsSubKey -Name 'MINK')
    Assert-Equal -What 'the file type itself is still registered' -Expected $ProgId `
        -Actual (Get-KeyValue -SubKey $ExtensionSubKey)

    $run = Invoke-RegisterScript -Arguments @('-Verbs', 'run') -ScriptPath $packageScript
    Assert-Equal -What 'registration with only the run verb exits 0' -Expected 0 -Actual $run.ExitCode
    Assert-True -What 'run is present' -Condition (Test-KeyExists -SubKey "$ShellSubKey\run")
    Assert-True -What 'edit is added because it is the safe double-click default' `
        -Condition (Test-KeyExists -SubKey "$ShellSubKey\edit")
    Assert-True -What 'check is absent' -Condition (-not (Test-KeyExists -SubKey "$ShellSubKey\check"))
    Assert-True -What 'build is absent' -Condition (-not (Test-KeyExists -SubKey "$ShellSubKey\build"))
    Assert-Equal -What 'the default verb is still edit' -Expected 'edit' -Actual (Get-KeyValue -SubKey $ShellSubKey)

    # Back to everything on, then execute the registered check verb with the real compiler.
    $run = Invoke-RegisterScript -Arguments @() -ScriptPath $packageScript
    Assert-Equal -What 'registration with everything on exits 0' -Expected 0 -Actual $run.ExitCode
    $realMink = Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'target\debug\mink.exe'
    if (Test-Path -LiteralPath $realMink) {
        $run = Invoke-RegisterScript -Arguments @('-MinkExe', $realMink) -ScriptPath $packageScript
        $verbCommand = (Get-KeyValue -SubKey "$ShellSubKey\check\command") -replace '%1', $helloMink
        $verbOutput = & cmd.exe /d /c "$verbCommand < nul" 2>&1 | Out-String
        Assert-Equal -What 'the check verb command exits 0 on a valid file' -Expected 0 -Actual $LASTEXITCODE
        Assert-True -What 'the check verb prints its header and the compiler result' `
            -Condition ($verbOutput -match 'MINK check:' -and $verbOutput -match 'passed parsing')
        Assert-True -What 'running the check verb did not build or run the file' `
            -Condition (-not (Test-Path -LiteralPath (Join-Path $package 'hello.exe')))
        $run = Invoke-RegisterScript -Arguments @() -ScriptPath $packageScript
    }
    else { Write-Note 'target\debug\mink.exe not built; the verb execution check is skipped' }

    # ---------------------------------------------------------------- step 6c
    Write-Step '6c. -Status reports the state and -Refresh keeps the choices when the package moves'
    $run = Invoke-RegisterScript -Arguments @('-Verbs', 'run', '-NoNewMenu', '-NoPreview') -ScriptPath $packageScript
    Assert-Equal -What 'a customised registration exits 0' -Expected 0 -Actual $run.ExitCode
    $run = Invoke-RegisterScript -Arguments @('-Status') -ScriptPath $packageScript
    Assert-Equal -What '-Status exits 0' -Expected 0 -Actual $run.ExitCode
    $status = @{}
    foreach ($line in ($run.Output -split "`n")) {
        if ($line -match '^([a-z_]+)=(.*)$') { $status[$Matches[1]] = $Matches[2].Trim() }
    }
    Assert-Equal -What '-Status says registered' -Expected 'true' -Actual $status['registered']
    Assert-Equal -What '-Status lists the verbs' -Expected 'run,edit' -Actual $status['verbs']
    Assert-Equal -What '-Status reports the default verb' -Expected 'edit' -Actual $status['default_verb']
    Assert-Equal -What '-Status reports the New menu as off' -Expected 'false' -Actual $status['new_menu']
    Assert-Equal -What '-Status reports preview as off' -Expected 'false' -Actual $status['preview']
    Assert-Equal -What '-Status reports Default apps as on' -Expected 'true' -Actual $status['default_apps']
    Assert-Equal -What '-Status reports the installed mink.exe' -Expected $installExe -Actual $status['mink_exe']
    Assert-True -What '-Status changed nothing' -Condition (-not (Test-KeyExists -SubKey $NewMenuSubKey))

    $moved = Join-Path $work 'moved package'
    Copy-Item -Path $package -Destination $moved -Recurse -Force
    $movedScript = Join-Path $moved 'windows\register-mink-filetype.ps1'
    $movedExe = (Resolve-Path -LiteralPath (Join-Path $moved 'bin\mink.exe')).Path
    $movedIcon = (Resolve-Path -LiteralPath (Join-Path $moved 'bin\mink-file.ico')).Path
    $run = Invoke-RegisterScript -Arguments @('-Refresh', '-Quiet') -ScriptPath $movedScript
    Assert-Equal -What '-Refresh from the moved package exits 0' -Expected 0 -Actual $run.ExitCode
    Assert-Equal -What 'the run entry now points at the moved mink.exe' `
        -Expected ('"{0}" shell run "%1"' -f $movedExe) -Actual (Get-KeyValue -SubKey "$ShellSubKey\run\command")
    Assert-Equal -What 'the icon now points at the moved package' `
        -Expected ('"{0}",0' -f $movedIcon) -Actual (Get-KeyValue -SubKey $DefaultIconSubKey)
    Assert-True -What 'the right-click entries that were off stay off' `
        -Condition (-not (Test-KeyExists -SubKey "$ShellSubKey\check") -and -not (Test-KeyExists -SubKey "$ShellSubKey\build"))
    Assert-True -What 'the New menu that was off stays off' -Condition (-not (Test-KeyExists -SubKey $NewMenuSubKey))
    Assert-True -What 'the preview handler that was off stays off' `
        -Condition (-not (Test-KeyExists -SubKey $PersistentHandlerSubKey))
    Assert-True -What 'Default apps that was on stays on' -Condition (Test-KeyExists -SubKey $CapabilitiesSubKey)
    Assert-True -What '-Refresh produced no output with -Quiet' -Condition (-not $run.Output.Trim())

    $run = Invoke-RegisterScript -Arguments @('-Uninstall', '-Quiet') -ScriptPath $movedScript
    Assert-Equal -What 'uninstall from the moved package exits 0' -Expected 0 -Actual $run.ExitCode
    $run = Invoke-RegisterScript -Arguments @('-Refresh') -ScriptPath $movedScript
    Assert-Equal -What '-Refresh with nothing registered exits 0' -Expected 0 -Actual $run.ExitCode
    Assert-True -What '-Refresh with nothing registered registers nothing' `
        -Condition (-not (Test-KeyExists -SubKey $ProgIdSubKey))
    Assert-True -What '-Refresh says there is nothing to refresh' -Condition ($run.Output -match 'nothing to refresh')
    # Back to a fully registered state for the steps that follow.
    $run = Invoke-RegisterScript -Arguments @() -ScriptPath $packageScript
    Assert-Equal -What 'registration for the next steps exits 0' -Expected 0 -Actual $run.ExitCode

    # ---------------------------------------------------------------- step 6d
    Write-Step '6d. mink-path.ps1 adds and removes a folder on the per-user PATH'
    $pathScript = Join-Path $package 'windows\mink-path.ps1'
    $envKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    $pathRaw = [string]$envKey.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $pathKind = if ($envKey.GetValueNames() -contains 'Path') { $envKey.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
    $envKey.Dispose()
    $probeDir = Join-Path $work 'path probe dir'
    try {
        $run = Invoke-RegisterScript -Arguments @('-Status', '-Directory', $probeDir) -ScriptPath $pathScript
        Assert-Equal -What 'the folder is not on PATH to begin with' -Expected 'on_path=false' -Actual $run.Output.Trim()
        $run = Invoke-RegisterScript -Arguments @('-Add', '-Directory', $probeDir) -ScriptPath $pathScript
        Assert-Equal -What '-Add exits 0' -Expected 0 -Actual $run.ExitCode
        $afterAdd = [string][Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment').GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        Assert-True -What 'the folder is now the last PATH entry' -Condition ($afterAdd.EndsWith($probeDir))
        Assert-True -What 'every earlier PATH entry is unchanged' -Condition ($afterAdd.StartsWith($pathRaw))
        $run = Invoke-RegisterScript -Arguments @('-Add', '-Directory', ($probeDir.ToUpper() + '\')) -ScriptPath $pathScript
        Assert-True -What 'adding it again (other case, trailing slash) is recognised' -Condition ($run.Output -match 'already on your PATH')
        $afterSecond = [string][Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment').GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        Assert-Equal -What 'the second add changed nothing' -Expected $afterAdd -Actual $afterSecond
        $run = Invoke-RegisterScript -Arguments @('-Status', '-Directory', $probeDir) -ScriptPath $pathScript
        Assert-Equal -What '-Status sees it' -Expected 'on_path=true' -Actual $run.Output.Trim()
        $run = Invoke-RegisterScript -Arguments @('-Remove', '-Directory', $probeDir) -ScriptPath $pathScript
        Assert-Equal -What '-Remove exits 0' -Expected 0 -Actual $run.ExitCode
        $afterRemove = [string][Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment').GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        Assert-Equal -What 'PATH is exactly what it was before' -Expected $pathRaw.TrimEnd(';') -Actual $afterRemove.TrimEnd(';')
        $run = Invoke-RegisterScript -Arguments @('-Add', '-Remove', '-Directory', $probeDir) -ScriptPath $pathScript
        Assert-Equal -What 'asking for two actions at once is refused' -Expected 1 -Actual $run.ExitCode
    }
    finally {
        # Whatever happened, the user's PATH goes back to the exact original text and type.
        $restore = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
        $restore.SetValue('Path', $pathRaw, $pathKind)
        $restore.Dispose()
    }
    Assert-Equal -What 'the original PATH text and type are restored' -Expected $pathRaw `
        -Actual ([string][Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment').GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames))

    # ---------------------------------------------------------------- step 6e
    Write-Step '6e. mink setup (the real compiler) drives the script'
    $realMinkForSetup = Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'target\debug\mink.exe'
    if (Test-Path -LiteralPath $realMinkForSetup) {
        $e2e = Join-Path $work 'setup package'
        Copy-Item -Path $package -Destination $e2e -Recurse -Force
        Copy-Item -LiteralPath $realMinkForSetup -Destination (Join-Path $e2e 'bin\mink.exe') -Force
        $e2eExe = (Resolve-Path -LiteralPath (Join-Path $e2e 'bin\mink.exe')).Path
        $e2eIcon = (Resolve-Path -LiteralPath (Join-Path $e2e 'bin\mink-file.ico')).Path
        $null = Invoke-RegisterScript -Arguments @('-Uninstall', '-Quiet') -ScriptPath $packageScript

        $out = & $e2eExe setup --status 2>&1 | Out-String
        Assert-True -What 'setup --status says not set up' -Condition ($out -match 'not set up')

        $out = & $e2eExe setup --yes --no-path --editor notepad 2>&1 | Out-String
        Assert-Equal -What 'setup --yes exits 0' -Expected 0 -Actual $LASTEXITCODE
        Assert-Equal -What 'setup registered .mink' -Expected $ProgId -Actual (Get-KeyValue -SubKey $ExtensionSubKey)
        Assert-Equal -What 'setup stored the icon of this package' -Expected ('"{0}",0' -f $e2eIcon) `
            -Actual (Get-KeyValue -SubKey $DefaultIconSubKey)
        Assert-Equal -What 'the run entry runs this mink.exe' -Expected ('"{0}" shell run "%1"' -f $e2eExe) `
            -Actual (Get-KeyValue -SubKey "$ShellSubKey\run\command")
        Assert-Equal -What 'double-click is the safe edit verb' -Expected 'edit' -Actual (Get-KeyValue -SubKey $ShellSubKey)

        $out = & $e2eExe setup --status 2>&1 | Out-String
        Assert-True -What 'setup --status says set up, with the right-click entries' `
            -Condition ($out -match '\.mink file type : set up' -and $out -match 'Run, Check, Build, Edit')

        $out = & $e2eExe setup --yes --no-path 2>&1 | Out-String
        $conflictSnapshot = Get-KeyValue -SubKey $ExtensionSubKey
        Assert-Equal -What 'running setup again keeps the file type' -Expected $ProgId -Actual $conflictSnapshot

        # --auto refreshes without a word and keeps the choices.
        $null = Invoke-RegisterScript -Arguments @('-NoNewMenu', '-Verbs', 'run') -ScriptPath (Join-Path $e2e 'windows\register-mink-filetype.ps1')
        $out = & $e2eExe setup --auto 2>&1 | Out-String
        Assert-Equal -What 'setup --auto exits 0' -Expected 0 -Actual $LASTEXITCODE
        Assert-Equal -What 'setup --auto prints nothing' -Expected '' -Actual $out.Trim()
        Assert-True -What 'setup --auto kept the New menu off' -Condition (-not (Test-KeyExists -SubKey $NewMenuSubKey))
        Assert-True -What 'setup --auto kept the entries that were off, off' `
            -Condition (-not (Test-KeyExists -SubKey "$ShellSubKey\check"))

        $out = & $e2eExe setup --remove 2>&1 | Out-String
        Assert-Equal -What 'setup --remove exits 0' -Expected 0 -Actual $LASTEXITCODE
        Assert-True -What 'setup --remove left no MINK ProgID' -Condition (-not (Test-KeyExists -SubKey $ProgIdSubKey))

        # --auto never takes .mink over from another program.
        Set-KeyValue -SubKey $ExtensionSubKey -Name '' -Value 'Contoso.SourceFile'
        $out = & $e2eExe setup --auto 2>&1 | Out-String
        Assert-Equal -What 'setup --auto leaves another program''s association' -Expected 'Contoso.SourceFile' `
            -Actual (Get-KeyValue -SubKey $ExtensionSubKey)
        $out = & $e2eExe setup --yes --no-path 2>&1 | Out-String
        Assert-True -What 'setup --yes explains that .mink belongs to another program' -Condition ($out -match 'Contoso\.SourceFile')
        Assert-Equal -What 'setup --yes left it alone too' -Expected 'Contoso.SourceFile' -Actual (Get-KeyValue -SubKey $ExtensionSubKey)
        Remove-KeyIfPresent -SubKey $ExtensionSubKey
        Remove-KeyIfPresent -SubKey $ProgIdSubKey
        $null = Invoke-RegisterScript -Arguments @() -ScriptPath $packageScript
    }
    else { Write-Note 'target\debug\mink.exe not built; the mink setup checks are skipped' }

    # ---------------------------------------------------------------- step 6f
    Write-Step '6f. the installer (mink installer) installs, upgrades and removes end to end'
    if (Test-Path -LiteralPath $realMinkForSetup) {
        # Native programs that print errors must not stop the script; their exit codes are asserted.
        $ErrorActionPreference = 'Continue'
        $repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
        function New-TestPayload {
            param([string]$Dir, [string]$Version)
            if (Test-Path -LiteralPath $Dir) { Remove-Item -LiteralPath $Dir -Recurse -Force }
            New-Item -ItemType Directory -Force -Path (Join-Path $Dir 'bin') | Out-Null
            Copy-Item -LiteralPath $realMinkForSetup -Destination (Join-Path $Dir 'bin\mink.exe')
            Copy-Item -LiteralPath (Join-Path $repoRoot 'stdlib') -Destination (Join-Path $Dir 'stdlib') -Recurse
            Copy-Item -LiteralPath $KitDir -Destination (Join-Path $Dir 'windows') -Recurse
            Copy-Item -LiteralPath (Join-Path $repoRoot 'LICENSE') -Destination (Join-Path $Dir 'LICENSE')
            & $realMinkForSetup installer --make-manifest $Dir --version $Version | Out-Null
        }
        $null = Invoke-RegisterScript -Arguments @('-Uninstall', '-Quiet') -ScriptPath $packageScript
        $payloadOne = Join-Path $work 'payload one'
        $payloadTwo = Join-Path $work 'payload two'
        New-TestPayload -Dir $payloadOne -Version '1.0.5'
        New-TestPayload -Dir $payloadTwo -Version '1.0.6'
        $installRoot = Join-Path $work 'installed root'
        $payloadExe = Join-Path $payloadOne 'bin\mink.exe'

        & $payloadExe installer --silent --payload $payloadOne --dir $installRoot --no-path --editor notepad 2>&1 | Out-Null
        Assert-Equal -What 'installer --silent exits 0' -Expected 0 -Actual $LASTEXITCODE
        Assert-True -What 'the version folder, launcher, kit and record exist' `
            -Condition ((Test-Path (Join-Path $installRoot 'versions\1.0.5\bin\mink.exe')) -and (Test-Path (Join-Path $installRoot 'bin\mink.exe')) -and
                (Test-Path (Join-Path $installRoot 'windows\register-mink-filetype.ps1')) -and (Test-Path (Join-Path $installRoot 'mink-install.json')))
        $shim = (Resolve-Path -LiteralPath (Join-Path $installRoot 'bin\mink.exe')).Path
        $stableIcon = (Resolve-Path -LiteralPath (Join-Path $installRoot 'windows\mink-file.ico')).Path
        Assert-Equal -What 'the installer registered .mink' -Expected $ProgId -Actual (Get-KeyValue -SubKey $ExtensionSubKey)
        Assert-Equal -What 'the icon is the stable copy in the install folder, not a version folder' `
            -Expected ('"{0}",0' -f $stableIcon) -Actual (Get-KeyValue -SubKey $DefaultIconSubKey)
        Assert-Equal -What 'the right-click entries run the launcher' -Expected ('"{0}" shell run "%1"' -f $shim) `
            -Actual (Get-KeyValue -SubKey "$ShellSubKey\run\command")
        Assert-Equal -What 'Apps & features lists MINK 1.0.5' -Expected '1.0.5' -Actual (Get-KeyValue -SubKey $UninstallSubKey -Name 'DisplayVersion')
        Assert-Equal -What 'the uninstall command runs the launcher' -Expected ('"{0}" installer --uninstall' -f $shim) `
            -Actual (Get-KeyValue -SubKey $UninstallSubKey -Name 'UninstallString')
        Assert-Equal -What 'the launcher runs and reports the version' -Expected ('mink ' + (& $realMinkForSetup --version | ForEach-Object { $_ -replace '^mink ', '' })) `
            -Actual ((& $shim --version) -join '')

        # An upgrade keeps the old version; the launcher follows the new one.
        $payloadTwoExe = Join-Path $payloadTwo 'bin\mink.exe'
        & $payloadTwoExe installer --silent --payload $payloadTwo --dir $installRoot --no-path --editor notepad 2>&1 | Out-Null
        Assert-Equal -What 'upgrading exits 0' -Expected 0 -Actual $LASTEXITCODE
        $listing = (& $shim versions) -join "`n"
        Assert-True -What 'mink versions lists both, with the new one as the default' `
            -Condition ($listing -match '1\.0\.6\s+\(default' -and $listing -match '1\.0\.5')
        Assert-Equal -What 'Apps & features now says 1.0.6' -Expected '1.0.6' -Actual (Get-KeyValue -SubKey $UninstallSubKey -Name 'DisplayVersion')
        $pinDir = Join-Path $work 'pinned project'
        New-Item -ItemType Directory -Force -Path $pinDir | Out-Null
        Set-Content -Path (Join-Path $pinDir '.mink-version') -Value '1.0.5' -Encoding Ascii
        Push-Location $pinDir
        $pinned = (& $shim versions) -join "`n"
        Pop-Location
        Assert-True -What 'a .mink-version file pins the version in that folder' -Condition ($pinned -match '\* 1\.0\.5')
        & $shim use 1.0.5 | Out-Null
        Assert-True -What 'mink use switches the default' -Condition (((& $shim versions) -join "`n") -match '1\.0\.5\s+\(default')

        # A folder with someone else's files is refused and left alone.
        $foreign = Join-Path $work 'foreign folder'
        New-Item -ItemType Directory -Force -Path $foreign | Out-Null
        Set-Content -Path (Join-Path $foreign 'mine.txt') -Value 'keep' -Encoding Ascii
        & $payloadExe installer --silent --payload $payloadOne --dir $foreign --no-association --no-path 2>&1 | Out-Null
        Assert-Equal -What 'a folder that holds other files is refused with exit code 6' -Expected 6 -Actual $LASTEXITCODE
        Assert-Equal -What 'and nothing was added to it' -Expected 1 -Actual @(Get-ChildItem -LiteralPath $foreign).Count

        # A damaged payload is refused before anything is written.
        $damaged = Join-Path $work 'payload damaged'
        Copy-Item -LiteralPath $payloadOne -Destination $damaged -Recurse
        Add-Content -Path (Join-Path $damaged 'LICENSE') -Value 'tampered'
        $damagedRoot = Join-Path $work 'damaged root'
        & (Join-Path $damaged 'bin\mink.exe') installer --silent --payload $damaged --dir $damagedRoot --no-association --no-path 2>&1 | Out-Null
        Assert-Equal -What 'a damaged payload exits 7' -Expected 7 -Actual $LASTEXITCODE
        Assert-True -What 'and creates nothing' -Condition (-not (Test-Path -LiteralPath $damagedRoot))

        # Removal from the launcher: the copy in temp finishes the job.
        $removalStart = (Get-Date).AddSeconds(-2)
        & $shim installer --uninstall --silent 2>&1 | Out-Null
        Assert-Equal -What 'installer --uninstall --silent exits 0' -Expected 0 -Actual $LASTEXITCODE
        $deadline = (Get-Date).AddSeconds(30)
        while ((Test-Path -LiteralPath $installRoot) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 250 }
        Assert-True -What 'the install folder is gone' -Condition (-not (Test-Path -LiteralPath $installRoot))
        Assert-True -What 'the .mink file type is gone' -Condition (-not (Test-KeyExists -SubKey $ProgIdSubKey) -and -not (Test-KeyExists -SubKey $ExtensionSubKey))
        Assert-True -What 'MINK is gone from Apps & features' -Condition (-not (Test-KeyExists -SubKey $UninstallSubKey))
        $leftover = @()
        $tempDeadline = (Get-Date).AddSeconds(15)
        do {
            $leftover = @(Get-ChildItem -LiteralPath $env:TEMP -Directory -Filter 'mink-uninstall-*' -ErrorAction SilentlyContinue |
                Where-Object { $_.CreationTime -ge $removalStart })
            if ($leftover.Count -gt 0) { Start-Sleep -Milliseconds 300 }
        } while ($leftover.Count -gt 0 -and (Get-Date) -lt $tempDeadline)
        Assert-Equal -What 'no temporary removal folder is left behind' -Expected 0 -Actual $leftover.Count
        $null = Invoke-RegisterScript -Arguments @() -ScriptPath $packageScript
        $ErrorActionPreference = 'Stop'
    }
    else { Write-Note 'target\debug\mink.exe not built; the installer checks are skipped' }

    # ---------------------------------------------------------------- step 7
    Write-Step '7. open behaviour with an editor bound'
    $probeExe = Join-Path $package 'probe-editor.exe'
    Add-Type -TypeDefinition @'
using System;
using System.IO;
public static class MinkFiletypeProbe
{
    public static void Main(string[] args)
    {
        string log = Environment.GetEnvironmentVariable("MINK_PROBE_LOG");
        if (string.IsNullOrEmpty(log)) { return; }
        File.AppendAllText(log, "argv=" + string.Join("|", args)
            + Environment.NewLine + "cwd=" + Directory.GetCurrentDirectory() + Environment.NewLine);
    }
}
'@ -OutputAssembly $probeExe -OutputType ConsoleApplication
    Assert-True -What 'probe editor built' -Condition (Test-Path -LiteralPath $probeExe)

    $run = Invoke-RegisterScript -Arguments @('-Editor', $probeExe) -ScriptPath $packageScript
    Assert-Equal -What 'registration with -Editor exits 0' -Expected 0 -Actual $run.ExitCode
    Assert-Equal -What 'open command is the editor applied to the file, quoted' `
        -Expected ('"{0}" "%1"' -f $probeExe) -Actual (Get-KeyValue -SubKey $OpenCommandSubKey)
    Assert-Equal -What 'with an editor bound, open is the double-click default' -Expected 'open' `
        -Actual (Get-KeyValue -SubKey $ShellSubKey)
    Assert-True -What 'a separate edit entry is not added next to the editor open entry' `
        -Condition (-not (Test-KeyExists -SubKey "$ShellSubKey\edit"))
    Assert-True -What 'run, check and build are still there' `
        -Condition ((Test-KeyExists -SubKey "$ShellSubKey\run") -and (Test-KeyExists -SubKey "$ShellSubKey\check") `
            -and (Test-KeyExists -SubKey "$ShellSubKey\build"))

    if (Test-Path -LiteralPath $probeLog) { Remove-Item -LiteralPath $probeLog -Force }
    $env:MINK_PROBE_LOG = $probeLog
    Start-Process -FilePath $helloMink
    $deadline = (Get-Date).AddSeconds(15)
    while (-not (Test-Path -LiteralPath $probeLog) -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 200
    }
    $logText = if (Test-Path -LiteralPath $probeLog) { (Get-Content -LiteralPath $probeLog -Raw) } else { '' }
    Assert-True -What 'opening the file through the association reached the editor' `
        -Condition ($logText -match [regex]::Escape('hello.mink'))
    Write-Note ("probe log: " + ($logText -replace "`r?`n", ' ').Trim())
    Assert-True -What 'the source file was not compiled or executed' `
        -Condition (-not (Test-Path -LiteralPath (Join-Path $package 'hello.exe')) -and
        -not (Test-Path -LiteralPath (Join-Path $work 'hello.exe')))

    # ---------------------------------------------------------------- step 8
    Write-Step '8. rebinding without -Editor removes the stale command'
    $run = Invoke-RegisterScript -Arguments @() -ScriptPath $packageScript
    Assert-Equal -What 're-registration exits 0' -Expected 0 -Actual $run.ExitCode
    Assert-True -What 'open command removed again' -Condition (-not (Test-KeyExists -SubKey $OpenCommandSubKey))
    Assert-Equal -What 'ProgID keeps only its own subkeys' -Expected 'DefaultIcon,shell' `
        -Actual ((Get-KeySubNames -SubKey $ProgIdSubKey) -join ',')
    Assert-Equal -What 'double-click is back to the safe edit verb' -Expected 'edit' `
        -Actual (Get-KeyValue -SubKey $ShellSubKey)

    # ---------------------------------------------------------------- step 9
    Write-Step '9. unicode installation path (and a path with a space)'
    # The whole package tree is copied under a Unicode directory; the script is run with no
    # arguments, so it must resolve mink.exe and mink-file.ico from its own location. The
    # path is never passed on a command line, which also keeps the test independent of the
    # console code page.
    # The name is built from code points so this file stays pure ASCII: Windows
    # PowerShell 5.1 reads a BOM-less .ps1 in the ANSI code page, which would corrupt
    # a non-ASCII literal in the source.
    $unicodeName = 'mink-' + [string][char]0x0442 + [string][char]0x0435 + [string][char]0x0441 +
        [string][char]0x0442 + '-' + [string][char]0x65E5 + [string][char]0x672C + [string][char]0x8A9E
    $unicodeRoot = Join-Path $work $unicodeName
    New-Item -ItemType Directory -Path $unicodeRoot -Force | Out-Null
    Copy-Item -Path $package -Destination $unicodeRoot -Recurse -Force
    $unicodeBin = Join-Path $unicodeRoot (Join-Path (Split-Path -Leaf $package) 'bin')
    $unicodeIcon = (Resolve-Path -LiteralPath (Join-Path $unicodeBin 'mink-file.ico')).Path
    $unicodeScript = Join-Path $unicodeRoot (Join-Path (Split-Path -Leaf $package) 'windows\register-mink-filetype.ps1')
    $run = Invoke-RegisterScript -Arguments @() -ScriptPath $unicodeScript
    Assert-Equal -What 'registration from a unicode path exits 0' -Expected 0 -Actual $run.ExitCode
    if ($run.ExitCode -ne 0) { Show-Output -Output $run.Output }
    Assert-Equal -What 'unicode icon path round-trips through the registry' `
        -Expected ('"{0}",0' -f $unicodeIcon) -Actual (Get-KeyValue -SubKey $DefaultIconSubKey)
    Assert-Equal -What 'the shell reports the unicode icon path' `
        -Expected ('"{0}",0' -f $unicodeIcon) `
        -Actual (Get-AssocString -Association $Extension -StringType $ASSOCSTR_DEFAULTICON)

    # ---------------------------------------------------------------- step 10
    Write-Step '10. uninstall from a clean extension removes everything MINK owns'
    $run = Invoke-RegisterScript -Arguments @('-Uninstall')
    Assert-Equal -What 'uninstall exits 0' -Expected 0 -Actual $run.ExitCode
    if ($run.ExitCode -ne 0) { Show-Output -Output $run.Output }
    Assert-True -What 'no MINK ProgID left behind' -Condition (-not (Test-KeyExists -SubKey $ProgIdSubKey))
    Assert-True -What 'no empty .mink key left behind' -Condition (-not (Test-KeyExists -SubKey $ExtensionSubKey))
    Assert-True -What 'MINK removed from "Open with"' `
        -Condition (-not ((Get-KeyValueNames -SubKey $OpenWithSubKey) -contains $ProgId))
    Assert-True -What 'no New-menu entry left behind' -Condition (-not (Test-KeyExists -SubKey $NewMenuSubKey))
    Assert-True -What 'no search filter left behind' -Condition (-not (Test-KeyExists -SubKey $PersistentHandlerSubKey))
    Assert-True -What 'no preview handler left behind' -Condition (-not (Test-KeyExists -SubKey $PreviewSubKey))
    Assert-True -What 'no Default apps entry left behind' -Condition (-not (Test-KeyExists -SubKey $MinkAppSubKey))
    Assert-Equal -What 'no RegisteredApplications value left behind' -Expected $null `
        -Actual (Get-KeyValue -SubKey $RegisteredAppsSubKey -Name 'MINK')

    # ---------------------------------------------------------------- step 11
    Write-Step '11. npm package layout: the icon lives in windows\, not in bin\'
    # This is the layout a real install has: <package>\bin\mink.exe plus the kit in
    # <package>\windows. The icon must be found from the script's own directory.
    $npmPackage = Join-Path $work 'npm-layout'
    $npmWindows = Join-Path $npmPackage 'windows'
    $npmBin = Join-Path $npmPackage 'bin'
    New-Item -ItemType Directory -Path $npmWindows -Force | Out-Null
    New-Item -ItemType Directory -Path $npmBin -Force | Out-Null
    Copy-Item -Path (Join-Path $KitDir '*') -Destination $npmWindows -Force
    Set-Content -Path (Join-Path $npmBin 'mink.exe') -Value 'stand-in' -Encoding Ascii
    $npmScript = Join-Path $npmWindows 'register-mink-filetype.ps1'
    $npmIcon = (Resolve-Path -LiteralPath (Join-Path $npmWindows 'mink-file.ico')).Path

    Remove-KeyIfPresent -SubKey $ExtensionSubKey
    Remove-KeyIfPresent -SubKey $ProgIdSubKey
    $run = Invoke-RegisterScript -Arguments @() -ScriptPath $npmScript
    Assert-Equal -What 'registration from the package layout exits 0' -Expected 0 -Actual $run.ExitCode
    if ($run.ExitCode -ne 0) { Show-Output -Output $run.Output }
    Assert-Equal -What 'the icon is the copy shipped in windows\' `
        -Expected ('"{0}",0' -f $npmIcon) -Actual (Get-KeyValue -SubKey $DefaultIconSubKey)
    Assert-True -What 'the resolved icon is a valid 7-frame ICO' `
        -Condition ((Get-IconFrameCount -Path $npmIcon) -eq 7)
    Assert-True -What 'the registered icon path is inside the package' `
        -Condition (& { param($icon, $root) $icon.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase) } $npmIcon $npmPackage)

    # ---------------------------------------------------------------- step 12
    Write-Step '12. source checkout: no installation present, icon beside the script'
    $checkout = Join-Path $work 'checkout\npm\mink\windows'
    New-Item -ItemType Directory -Path $checkout -Force | Out-Null
    Copy-Item -Path (Join-Path $KitDir '*') -Destination $checkout -Force
    $checkoutScript = Join-Path $checkout 'register-mink-filetype.ps1'
    $checkoutIcon = (Resolve-Path -LiteralPath (Join-Path $checkout 'mink-file.ico')).Path

    Remove-KeyIfPresent -SubKey $ExtensionSubKey
    Remove-KeyIfPresent -SubKey $ProgIdSubKey
    $run = Invoke-RegisterScript -Arguments @() -ScriptPath $checkoutScript
    Assert-Equal -What 'registration from a source checkout exits 0' -Expected 0 -Actual $run.ExitCode
    if ($run.ExitCode -ne 0) { Show-Output -Output $run.Output }
    Assert-Equal -What 'the icon resolves beside the script, not from the working directory' `
        -Expected ('"{0}",0' -f $checkoutIcon) -Actual (Get-KeyValue -SubKey $DefaultIconSubKey)
    Assert-True -What 'no path outside the invocation tree is registered' `
        -Condition (& { param($icon, $root) $icon.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase) } $checkoutIcon $work)

    # ---------------------------------------------------------------- step 13
    Write-Step '13. a missing, empty or malformed icon refuses and changes nothing'
    $noIconWindows = Join-Path $work 'no-icon-package\windows'
    $noIconBin = Join-Path $work 'no-icon-package\bin'
    New-Item -ItemType Directory -Path $noIconWindows -Force | Out-Null
    New-Item -ItemType Directory -Path $noIconBin -Force | Out-Null
    Get-ChildItem -LiteralPath $KitDir -File |
        Where-Object { $_.Extension -ne '.ico' } |
        Copy-Item -Destination $noIconWindows -Force
    Set-Content -Path (Join-Path $noIconBin 'mink.exe') -Value 'stand-in' -Encoding Ascii
    $noIconScript = Join-Path $noIconWindows 'register-mink-filetype.ps1'

    # Start from a working registration so "nothing changed" is observable.
    Remove-KeyIfPresent -SubKey $ExtensionSubKey
    Remove-KeyIfPresent -SubKey $ProgIdSubKey
    $null = Invoke-RegisterScript -Arguments @() -ScriptPath $npmScript
    $extensionBefore = Get-RegistrySignature -SubKey $ExtensionSubKey
    $progIdBefore = Get-RegistrySignature -SubKey $ProgIdSubKey
    Assert-True -What 'baseline registration exists for the refusal checks' `
        -Condition ($extensionBefore -match 'MINK\.SourceFile')

    $run = Invoke-RegisterScript -Arguments @() -ScriptPath $noIconScript
    Assert-Equal -What 'a package without any ICO refuses' -Expected 1 -Actual $run.ExitCode
    Assert-True -What 'the refusal says no icon was found' `
        -Condition ($run.Output -match 'No mink-file\.ico found')
    Assert-Equal -What 'the extension is untouched after a missing icon' `
        -Expected $extensionBefore -Actual (Get-RegistrySignature -SubKey $ExtensionSubKey)
    Assert-Equal -What 'the ProgID is untouched after a missing icon' `
        -Expected $progIdBefore -Actual (Get-RegistrySignature -SubKey $ProgIdSubKey)

    $malformedIcon = Join-Path $work 'malformed.ico'
    Set-Content -Path $malformedIcon -Value 'this is not an icon' -Encoding Ascii
    $run = Invoke-RegisterScript -Arguments @('-IconPath', $malformedIcon) -ScriptPath $npmScript
    Assert-Equal -What 'a malformed -IconPath refuses' -Expected 1 -Actual $run.ExitCode
    Assert-True -What 'the refusal calls the file invalid' `
        -Condition ($run.Output -match 'not a valid \.ico')
    Assert-Equal -What 'the registry is untouched after a malformed icon' `
        -Expected $extensionBefore -Actual (Get-RegistrySignature -SubKey $ExtensionSubKey)

    $emptyIcon = Join-Path $work 'empty.ico'
    Set-Content -Path $emptyIcon -Value '' -Encoding Ascii
    $run = Invoke-RegisterScript -Arguments @('-IconPath', $emptyIcon) -ScriptPath $npmScript
    Assert-Equal -What 'an empty -IconPath refuses' -Expected 1 -Actual $run.ExitCode
    Assert-Equal -What 'the registry is untouched after an empty icon' `
        -Expected $extensionBefore -Actual (Get-RegistrySignature -SubKey $ExtensionSubKey)

    $run = Invoke-RegisterScript -Arguments @('-IconPath', (Join-Path $work 'absent.ico')) -ScriptPath $npmScript
    Assert-Equal -What 'a non-existent -IconPath refuses' -Expected 1 -Actual $run.ExitCode
    Assert-True -What 'the refusal names the missing path' `
        -Condition ($run.Output -match 'icon not found')
    Assert-Equal -What 'the registry is untouched after a missing -IconPath' `
        -Expected $extensionBefore -Actual (Get-RegistrySignature -SubKey $ExtensionSubKey)

    # ---------------------------------------------------------------- step 14
    Write-Step '14. shipped kit: complete, portable and one extension only'
    $kitText = Get-Content -LiteralPath $registerScript -Raw
    Assert-True -What 'the kit ships register-mink-filetype.ps1' `
        -Condition (Test-Path -LiteralPath $registerScript)
    Assert-True -What 'the kit ships mink-file.ico and mink-app.ico' `
        -Condition ((Test-Path -LiteralPath (Join-Path $KitDir 'mink-file.ico')) -and
        (Test-Path -LiteralPath (Join-Path $KitDir 'mink-app.ico')))
    Assert-True -What 'the kit ships template.mink for the New menu' `
        -Condition (Test-Path -LiteralPath (Join-Path $KitDir 'template.mink'))
    Assert-True -What 'the kit ships mink.rc and README.md' `
        -Condition ((Test-Path -LiteralPath (Join-Path $KitDir 'mink.rc')) -and
        (Test-Path -LiteralPath (Join-Path $KitDir 'README.md')))
    Assert-Equal -What 'mink-file.ico has 7 frames' `
        -Expected 7 -Actual (Get-IconFrameCount -Path (Join-Path $KitDir 'mink-file.ico'))
    Assert-Equal -What 'mink-app.ico has 7 frames' `
        -Expected 7 -Actual (Get-IconFrameCount -Path (Join-Path $KitDir 'mink-app.ico'))

    # `.mink` is the only extension MINK owns. mink.toml and mink.lock are identified by
    # file name, not by extension, so claiming .toml or .lock would take those types over
    # from unrelated applications. MinkFileType / UninstallMinkFileType are function names,
    # hence the `$Extension = ` anchor.
    $declaredExtensions = @([regex]::Matches($kitText, "(?m)^\`$Extension\s*=\s*'([^']+)'") |
        ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique)
    Assert-Equal -What 'the script declares exactly one extension' `
        -Expected '.mink' -Actual ($declaredExtensions -join ',')
    Assert-True -What 'the script never claims .toml, .lock or a wildcard' `
        -Condition ($kitText -notmatch '(?im)^\$Extension\s*=\s*''\.(toml|lock|\*)''')
    # Help text may document an editor path as an example, so strip comments before
    # asking whether the code itself assumes a drive or a development directory.
    $kitCode = [regex]::Replace($kitText, '(?s)<#.*?#>', '')
    $kitCode = [regex]::Replace($kitCode, '(?m)#.*$', '')
    Assert-True -What 'the script code contains no hard-coded drive path' `
        -Condition ($kitCode -notmatch '(?<![A-Za-z0-9])[A-Za-z]:\\')
    Assert-True -What 'the script resolves assets from its own location' `
        -Condition ($kitCode -match '\$PSScriptRoot')

    $manifestPath = Join-Path (Split-Path -Parent $KitDir) 'package.json'
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    Assert-True -What 'the npm manifest publishes windows\' `
        -Condition (@($manifest.files) -contains 'windows')
    Assert-True -What 'the npm manifest publishes bin/mink.exe' `
        -Condition (@($manifest.files) -contains 'bin/mink.exe')

    # ---------------------------------------------------------------- step 15
    Write-Step '15. previous machine state restored'
    Restore-MachineState
    $subKeys = @($ExtensionSubKey, $ProgIdSubKey, $FileExtsSubKey, $MinkAppSubKey, $null, $UninstallSubKey)
    $restored = $true
    if ("$(Get-KeyValue -SubKey $RegisteredAppsSubKey -Name 'MINK')" -ne "$($snapshots[4])") {
        $restored = $false
        Write-Note 'not restored: the MINK value under RegisteredApplications'
    }
    foreach ($index in @(0, 1, 2, 3, 5)) {
        if (-not (Test-Snapshot -SubKey $subKeys[$index] -Snapshot $snapshots[$index])) {
            $restored = $false
            Write-Note "not restored: $($subKeys[$index])"
        }
    }
    Assert-True -What 'the registry is back to its starting state' -Condition $restored
}
finally {
    Restore-MachineState
    if (-not $KeepWorkDirectory -and (Test-Path -LiteralPath $work)) {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Host ''
if ($script:failed -gt 0) {
    $result = 1
    Write-Host ("RESULT: FAIL - {0} passed, {1} failed" -f $script:passed, $script:failed) -ForegroundColor Red
}
else {
    Write-Host ("RESULT: PASS - {0} checks" -f $script:passed) -ForegroundColor Green
}
exit $result
